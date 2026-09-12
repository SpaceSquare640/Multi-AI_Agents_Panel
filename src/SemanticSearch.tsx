import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./shell/Icons";
import { ShellSidebar, useIsActiveScreen } from "./shell/SidebarSlot";
import type { Agent, MlAccessGrant, SemanticSearchResult, Session } from "./types";
import "./styles/screens/semantic-search.css";

/** One semantic-search index this app knows about, resolved from the raw
 *  per-agent/per-session grant data into everything a rebuild/search
 *  action needs — built once in `loadIndexes` so the render/action code
 *  below never has to re-derive `indexName`/`actingAgentId` itself. */
type IndexEntry = {
  key: string;
  scopeKind: "agent" | "session";
  scopeId: string;
  label: string;
  capabilityName: string;
  indexName: string;
  actingAgentId: string;
};

/** Semantic search, rebuilt against the v2 design.
 *
 *  The indexes move to the shell's context sidebar and one of them is
 *  selected at a time, which is the design's arrangement. The previous
 *  version stacked every index as its own card, each with its own
 *  rebuild button, search field and result list — so the same query had
 *  to be typed once per index, and the results of different indexes
 *  never appeared in the same place.
 *
 *  Three parts of the design are not here. Its match-mode toggle
 *  (meaning / exact) has no counterpart: `semantic_search_query` only
 *  matches by meaning. Its indexing meter needs progress events, and
 *  `build_semantic_index` is one call that returns when it has finished
 *  — a meter that jumps from nothing to complete would be decoration
 *  pretending to be information. And its chunk counts are not exposed by
 *  any command. */
export default function SemanticSearch() {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);

  const [indexes, setIndexes] = useState<IndexEntry[]>([]);
  const [loadingIndexes, setLoadingIndexes] = useState(true);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [indexingKey, setIndexingKey] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Record<string, SemanticSearchResult[]>>({});
  const [searchingKey, setSearchingKey] = useState<string | null>(null);
  const headerRef = useRef<HTMLDivElement>(null);
  const isActiveScreen = useIsActiveScreen(headerRef);

  async function loadIndexes() {
    setLoadingIndexes(true);
    try {
      const [agents, sessions] = await Promise.all([
        invoke<Agent[]>("list_agents"),
        invoke<Session[]>("list_sessions"),
      ]);
      const groupSessions = sessions.filter((s) => s.kind === "group");

      const agentEntries = (
        await Promise.all(
          agents.map(async (agent) => {
            const grants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_agent", { agentId: agent.id });
            return grants.map(
              (g): IndexEntry => ({
                key: `agent-${agent.id}-${g.capabilityName}`,
                scopeKind: "agent",
                scopeId: agent.id,
                label: agent.name,
                capabilityName: g.capabilityName,
                indexName: agent.id,
                actingAgentId: agent.id,
              }),
            );
          }),
        )
      ).flat();

      const sessionEntries = (
        await Promise.all(
          groupSessions.map(async (session) => {
            const grants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_session", {
              sessionId: session.id,
            });
            if (grants.length === 0) return [];
            const actingAgentId = await invoke<string | null>("get_session_agent_id", { sessionId: session.id });
            if (!actingAgentId) return [];
            return grants.map(
              (g): IndexEntry => ({
                key: `session-${session.id}-${g.capabilityName}`,
                scopeKind: "session",
                scopeId: session.id,
                label: session.title,
                capabilityName: g.capabilityName,
                indexName: `group-${session.id}`,
                actingAgentId,
              }),
            );
          }),
        )
      ).flat();

      const all = [...agentEntries, ...sessionEntries];
      setIndexes(all);
      /* Keep the current selection if it still exists — a refresh should
         not move someone off the index they were reading. */
      setSelectedKey((prev) => (prev && all.some((e) => e.key === prev) ? prev : (all[0]?.key ?? null)));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingIndexes(false);
    }
  }

  useEffect(() => {
    loadIndexes();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selected = indexes.find((e) => e.key === selectedKey) ?? null;

  async function handleRebuildIndex(entry: IndexEntry) {
    setError(null);
    setIndexingKey(entry.key);
    try {
      if (entry.scopeKind === "session") {
        await invoke("build_semantic_index_for_session", {
          sessionId: entry.scopeId,
          agentId: entry.actingAgentId,
          indexName: entry.indexName,
        });
      } else {
        await invoke("build_semantic_index", { agentId: entry.actingAgentId, indexName: entry.indexName });
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setIndexingKey(null);
    }
  }

  async function handleSearch(entry: IndexEntry) {
    const q = query.trim();
    if (!q) return;
    setError(null);
    setSearchingKey(entry.key);
    try {
      const result = await invoke<{ results: SemanticSearchResult[] }>("semantic_search_query", {
        agentId: entry.actingAgentId,
        indexName: entry.indexName,
        query: q,
        topK: 5,
      });
      setResults((prev) => ({ ...prev, [entry.key]: result.results }));
    } catch (err) {
      setError(String(err));
    } finally {
      setSearchingKey(null);
    }
  }

  const shown = selected ? results[selected.key] : undefined;

  return (
    <>
      {isActiveScreen && (
        <ShellSidebar v2>
          <div className="sidebar-header">
            <span className="label">{t("semanticSearch.indexes")}</span>
          </div>

          <div className="sidebar-body">
            {indexes.map((entry) => (
              <button
                className="row row-tall"
                type="button"
                key={entry.key}
                aria-current={entry.key === selectedKey ? "true" : undefined}
                onClick={() => setSelectedKey(entry.key)}
              >
                <Icon name={entry.scopeKind === "session" ? "chat" : "models"} size="sm" />
                <span className="name">
                  {entry.label}
                  <span className="row-sub">
                    {entry.scopeKind === "session" ? t("ml.semanticSearch.groupChat") : t("ml.semanticSearch.agent")} ·{" "}
                    {entry.capabilityName}
                  </span>
                </span>
              </button>
            ))}

            {/* The empty state is stated once, in the workspace. Saying it
                here as well put the same sentence twice on one screen.
                The note below is different information — what the index
                does and does not cover — so it stays. */}
            <p className="sidebar-note">{t("semanticSearch.grantedOnly")}</p>
          </div>
        </ShellSidebar>
      )}

      <div className="workspace-header" ref={headerRef}>
        <span className="workspace-title">{t("semanticSearch.title")}</span>
        <div className="workspace-actions">
          {/* Refresh is a labelled button here rather than an icon in the
              sidebar header: the design's sprite has no refresh glyph,
              because its sidebar header button adds a folder. Borrowing
              the magnifier for it would have said "search" on a control
              that re-reads the list. Usage and Skills already put Refresh
              in this slot. */}
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            disabled={loadingIndexes}
            onClick={() => void loadIndexes()}
          >
            {loadingIndexes ? t("skills.refreshing") : t("skills.refresh")}
          </button>
          {selected && (
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={indexingKey === selected.key}
              onClick={() => void handleRebuildIndex(selected)}
            >
              {indexingKey === selected.key ? t("chat.indexing") : t("chat.rebuildIndex")}
            </button>
          )}
        </div>
      </div>

      <div className="workspace-body">
        <div className="pane pane-wide">
          {error && (
            <div className="callout" data-kind="danger" role="alert">
              <div className="callout-body">{error}</div>
            </div>
          )}

          <p className="pane-intro">{t("ml.semanticSearch.hint")}</p>

          {!selected ? (
            <p className="pane-intro">{t("ml.semanticSearch.none")}</p>
          ) : (
            <>
              <form
                className="searchbar"
                onSubmit={(e) => {
                  e.preventDefault();
                  void handleSearch(selected);
                }}
              >
                <div className="input-group">
                  <Icon name="search" size="sm" />
                  <input
                    className="input"
                    type="search"
                    placeholder={t("chat.searchFilesPlaceholder")}
                    aria-label={t("chat.searchFilesPlaceholder")}
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </div>
                <button className="btn btn-primary" type="submit" disabled={searchingKey === selected.key}>
                  {searchingKey === selected.key ? t("chat.searching") : t("chat.search")}
                </button>
              </form>

              {shown && (
                <section>
                  <div className="section-head">
                    <h2>{t("semanticSearch.results")}</h2>
                    <span className="count">{t("semanticSearch.hitCount", { count: shown.length })}</span>
                    <span className="spacer" />
                    <span className="count">{t("semanticSearch.byMeaning")}</span>
                  </div>

                  {shown.length === 0 && <p className="pane-intro">{t("chat.noResults")}</p>}

                  {shown.map((r) => (
                    <div className="card hit" key={r.path}>
                      <div className="hit-head">
                        <Icon name="notes" size="sm" />
                        <span className="hit-path">{r.path}</span>
                        <span className="hit-score">{r.score.toFixed(2)}</span>
                      </div>
                      <p className="hit-snippet">{r.excerpt}</p>
                    </div>
                  ))}
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}
