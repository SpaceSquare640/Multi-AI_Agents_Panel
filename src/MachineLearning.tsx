import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Agent, MlAccessGrant, ModelRecommendation, SemanticSearchResult, Session } from "./types";
import "./MachineLearning.css";

/** One semantic-search index this app knows about, resolved from the raw
 *  per-agent/per-session grant data into everything a rebuild/search
 *  action needs — built once in `loadGrants` so the render/action code
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

export default function MachineLearning() {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);

  // --- Semantic search overview ---
  const [indexes, setIndexes] = useState<IndexEntry[]>([]);
  const [loadingIndexes, setLoadingIndexes] = useState(true);
  const [indexingKey, setIndexingKey] = useState<string | null>(null);
  const [searchQueries, setSearchQueries] = useState<Record<string, string>>({});
  const [searchResults, setSearchResults] = useState<Record<string, SemanticSearchResult[]>>({});
  const [searchingKey, setSearchingKey] = useState<string | null>(null);

  // --- Hardware fit (moved from AI Control Center) ---
  const [hardwareRecommendations, setHardwareRecommendations] = useState<ModelRecommendation[] | null>(null);
  const [loadingHardwareRecommendations, setLoadingHardwareRecommendations] = useState(false);

  // --- Game-Playing Agent, Track A (moved from AI Control Center) ---
  const [gameAgentRunning, setGameAgentRunning] = useState(false);
  const [gameAgentModel, setGameAgentModel] = useState("llava");
  const [gameAgentPrompt, setGameAgentPrompt] = useState(
    "You are playing a game. Look at the screenshot and decide the single best next action. " +
      'Reply with ONLY a JSON object: {"action":"click","x":<int>,"y":<int>} or ' +
      '{"action":"key","key":"<name>"} or {"action":"wait"}.',
  );
  const [gameAgentBusy, setGameAgentBusy] = useState(false);

  // --- Track B (Deep RL) recording (moved from AI Control Center) ---
  const [recording, setRecording] = useState(false);
  const [recordingSession, setRecordingSession] = useState("session-1");
  const [recordingOutputDir, setRecordingOutputDir] = useState("");
  const [recordingBusy, setRecordingBusy] = useState(false);

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

      setIndexes([...agentEntries, ...sessionEntries]);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingIndexes(false);
    }
  }

  useEffect(() => {
    loadIndexes();
    invoke<boolean>("game_agent_status").then(setGameAgentRunning).catch((e) => setError(String(e)));
    invoke<boolean>("recording_status").then(setRecording).catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
    const query = (searchQueries[entry.key] ?? "").trim();
    if (!query) return;
    setError(null);
    setSearchingKey(entry.key);
    try {
      const result = await invoke<{ results: SemanticSearchResult[] }>("semantic_search_query", {
        agentId: entry.actingAgentId,
        indexName: entry.indexName,
        query,
        topK: 5,
      });
      setSearchResults((prev) => ({ ...prev, [entry.key]: result.results }));
    } catch (err) {
      setError(String(err));
    } finally {
      setSearchingKey(null);
    }
  }

  async function handleRecommendLocalModels() {
    setError(null);
    setLoadingHardwareRecommendations(true);
    try {
      setHardwareRecommendations(await invoke<ModelRecommendation[]>("recommend_local_models", { limit: 8 }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingHardwareRecommendations(false);
    }
  }

  async function handleStartGameAgent() {
    setError(null);
    setGameAgentBusy(true);
    try {
      await invoke("start_game_agent", { model: gameAgentModel, prompt: gameAgentPrompt });
      setGameAgentRunning(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setGameAgentBusy(false);
    }
  }

  async function handleStopGameAgent() {
    setError(null);
    try {
      await invoke("stop_game_agent");
      setGameAgentRunning(false);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleStartRecording() {
    setError(null);
    setRecordingBusy(true);
    try {
      await invoke("start_recording_session", { session: recordingSession, outputDir: recordingOutputDir });
      setRecording(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setRecordingBusy(false);
    }
  }

  async function handleStopRecording() {
    setError(null);
    try {
      await invoke("stop_recording_session");
      setRecording(false);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="ml-page">
      <h1>{t("ml.title")}</h1>
      {error && (
        <div className="acc-error" role="alert">
          {error}
          <button onClick={() => setError(null)} aria-label={t("acc.dismissError")}>×</button>
        </div>
      )}

      <section className="acc-section">
        <div className="ml-section-head">
          <h2>{t("ml.semanticSearch.heading")}</h2>
          <button onClick={() => void loadIndexes()} disabled={loadingIndexes}>
            {loadingIndexes ? t("skills.refreshing") : t("skills.refresh")}
          </button>
        </div>
        <p className="acc-hint">{t("ml.semanticSearch.hint")}</p>

        {!loadingIndexes && indexes.length === 0 && <p className="acc-empty">{t("ml.semanticSearch.none")}</p>}

        {indexes.map((entry) => (
          <div className="ml-index-card" key={entry.key}>
            <div className="ml-index-card-head">
              <span className="ml-index-label">{entry.label}</span>
              <span className={entry.scopeKind === "session" ? "source-tag custom" : "source-tag builtin"}>
                {entry.scopeKind === "session" ? t("ml.semanticSearch.groupChat") : t("ml.semanticSearch.agent")}
              </span>
              <span className="acc-mono">{entry.capabilityName}</span>
            </div>
            <button disabled={indexingKey === entry.key} onClick={() => void handleRebuildIndex(entry)}>
              {indexingKey === entry.key ? t("chat.indexing") : t("chat.rebuildIndex")}
            </button>
            <div className="acc-form-row">
              <input
                type="text"
                placeholder={t("chat.searchFilesPlaceholder")}
                value={searchQueries[entry.key] ?? ""}
                onChange={(e) => setSearchQueries((prev) => ({ ...prev, [entry.key]: e.target.value }))}
              />
              <button disabled={searchingKey === entry.key} onClick={() => void handleSearch(entry)}>
                {searchingKey === entry.key ? t("chat.searching") : t("chat.search")}
              </button>
            </div>
            {searchResults[entry.key] && (
              <ul className="acc-model-list">
                {searchResults[entry.key].length === 0 && <li className="chat-empty">{t("chat.noResults")}</li>}
                {searchResults[entry.key].map((r) => (
                  <li key={r.path}>
                    <span className="acc-mono">{r.path}</span> ({r.score.toFixed(2)})
                    <p className="acc-hint">{r.excerpt}</p>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </section>

      <section className="acc-section">
        <h2>{t("acc.localModels.hardwareFitHeading")}</h2>
        <p className="acc-hint">{t("acc.localModels.hardwareFitHint")}</p>
        <button disabled={loadingHardwareRecommendations} onClick={() => void handleRecommendLocalModels()}>
          {loadingHardwareRecommendations ? t("acc.localModels.hardwareFitLoading") : t("acc.localModels.hardwareFitButton")}
        </button>
        {hardwareRecommendations && (
          <table className="acc-table">
            <thead>
              <tr>
                <th>{t("acc.localModels.tableModel")}</th>
                <th>{t("acc.localModels.hardwareFitLevel")}</th>
                <th>{t("acc.localModels.hardwareFitScore")}</th>
                <th>{t("acc.localModels.hardwareFitSpeed")}</th>
                <th>{t("acc.localModels.hardwareFitQuant")}</th>
              </tr>
            </thead>
            <tbody>
              {hardwareRecommendations.map((m) => (
                <tr key={m.name}>
                  <td className="acc-mono">
                    {m.name} ({m.parameterCount})
                  </td>
                  <td>{m.fitLevel}</td>
                  <td>{m.score.toFixed(0)}</td>
                  <td>{m.estimatedTokensPerSecond.toFixed(1)} tok/s</td>
                  <td className="acc-mono">{m.bestQuantization}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="acc-section">
        <h2>{t("acc.gameAgent.heading")}</h2>
        <p className="acc-hint">{t("acc.gameAgent.warning")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.gameAgent.modelPlaceholder")}
            value={gameAgentModel}
            onChange={(e) => setGameAgentModel(e.target.value)}
            disabled={gameAgentRunning}
          />
        </div>
        <textarea
          rows={3}
          value={gameAgentPrompt}
          onChange={(e) => setGameAgentPrompt(e.target.value)}
          disabled={gameAgentRunning}
        />
        <p>
          {t("acc.gameAgent.statusLabel")}{" "}
          {gameAgentRunning ? t("acc.gameAgent.statusRunning") : t("acc.gameAgent.statusStopped")}{" "}
          {gameAgentRunning ? (
            <button onClick={() => handleStopGameAgent()}>{t("acc.gameAgent.stop")}</button>
          ) : (
            <button disabled={gameAgentBusy} onClick={() => handleStartGameAgent()}>
              {gameAgentBusy ? t("acc.gameAgent.starting") : t("acc.gameAgent.start")}
            </button>
          )}
        </p>
      </section>

      <section className="acc-section">
        <h2>{t("acc.recording.heading")}</h2>
        <p className="acc-hint">{t("acc.recording.hint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.recording.sessionNamePlaceholder")}
            value={recordingSession}
            onChange={(e) => setRecordingSession(e.target.value)}
            disabled={recording}
          />
          <input
            type="text"
            placeholder={t("acc.recording.outputDirectoryPlaceholder")}
            value={recordingOutputDir}
            onChange={(e) => setRecordingOutputDir(e.target.value)}
            disabled={recording}
          />
        </div>
        <p>
          {t("acc.recording.statusLabel")}{" "}
          {recording ? t("acc.recording.statusRecording") : t("acc.recording.statusStopped")}{" "}
          {recording ? (
            <button onClick={() => handleStopRecording()}>{t("acc.recording.stop")}</button>
          ) : (
            <button disabled={recordingBusy || !recordingOutputDir.trim()} onClick={() => handleStartRecording()}>
              {recordingBusy ? t("acc.recording.starting") : t("acc.recording.startRecording")}
            </button>
          )}
        </p>
      </section>
    </div>
  );
}
