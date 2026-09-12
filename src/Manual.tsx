import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "./shell/Icons";
import { ShellSidebar, useIsActiveScreen } from "./shell/SidebarSlot";
import "./styles/screens/help.css";

export interface Article {
  id: string;
  title: string;
  category: string;
  minutes: number;
  paragraphs: string[];
}

/** Case-insensitive substring match against an article's title or any
 *  paragraph. Extracted from the component's `useMemo` so the matching
 *  rule (what counts as a hit) is testable independent of React state.
 *  Operates on already-resolved (translated) `Article[]` — per explicit
 *  user decision, search matches whatever language is currently
 *  displayed, not the original English source. */
export function filterArticles(articles: Article[], query: string): Article[] {
  const q = query.trim().toLowerCase();
  if (!q) return articles;
  return articles.filter(
    (a) => a.title.toLowerCase().includes(q) || a.paragraphs.some((p) => p.toLowerCase().includes(q)),
  );
}

/** Category ids in display order — kept as stable English identifiers
 *  internally (matching `manual.categoryLabels.<id>` in the translation
 *  file for the actual display text) so nothing here has to change
 *  when a translation changes what the category is called. */
const CATEGORY_ORDER = ["gettingStarted", "agents", "machineLearning", "safety"];

export default function Manual() {
  const { t } = useTranslation();
  /** The in-app User Manual's initial content — per Design Principles'
   *  decided scope: "獨立 Session / Group Chat 差異、如何設定本地模型、
   *  如何設定雲端 API Key、角色模板如何使用、Guardrails 是什麼". Real
   *  descriptions of what this app's already-shipped features actually
   *  do, not placeholder text — content itself lives in
   *  `manual.articles` in `src/locales/en/translation.json`. */
  const articles = t("manual.articles", { returnObjects: true }) as Article[];
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(articles[0].id);
  /* Anchored on the header rather than a wrapper of its own: the hook only
     needs somewhere inside this screen's pane to look upward from, and the
     v2 screen has no wrapper element — its header and body are direct
     children of the pane so the shell's own layout applies to them. */
  const headerRef = useRef<HTMLDivElement>(null);
  const isActiveScreen = useIsActiveScreen(headerRef);

  const filtered = useMemo(() => filterArticles(articles, query), [articles, query]);

  const selected = articles.find((a) => a.id === selectedId) ?? filtered[0] ?? articles[0];

  return (
    <>
      {/* The contents list is what the v2 layout calls context for the
          active rail item, so it lives in the shell's sidebar rather than
          in a column of this screen's own. Same portal Chat uses, and
          gated the same way — every screen stays mounted, so only the
          visible one may fill the shared region. */}
      {isActiveScreen && (
        <ShellSidebar v2>
          <div className="sidebar-header">
            <span className="label">{t("manual.title")}</span>
          </div>

          <div className="sidebar-body">
            <div className="input-group" style={{ margin: "var(--space-3) var(--space-2) var(--space-5)" }}>
              <Icon name="search" size="sm" />
              <input
                className="input"
                type="search"
                placeholder={t("manual.searchPlaceholder")}
                aria-label={t("manual.searchPlaceholder")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>

            {CATEGORY_ORDER.map((category) => {
              const inCategory = filtered.filter((a) => a.category === category);
              if (inCategory.length === 0) return null;
              return (
                <div key={category}>
                  <div className="label" style={{ padding: "var(--space-6) var(--row-px) var(--space-3)" }}>
                    {t(`manual.categoryLabels.${category}`)}
                  </div>
                  {inCategory.map((a) => (
                    <button
                      key={a.id}
                      className="row"
                      type="button"
                      /* aria-current rather than a class: it is what the
                         .row style keys off, and it says "this is the one
                         you are reading" to a screen reader too, which a
                         class cannot. */
                      aria-current={a.id === selected.id ? "true" : undefined}
                      onClick={() => setSelectedId(a.id)}
                    >
                      <span className="name">{a.title}</span>
                    </button>
                  ))}
                </div>
              );
            })}

            {filtered.length === 0 && (
              <p className="label" style={{ padding: "var(--space-6) var(--row-px)", textTransform: "none" }}>
                {t("manual.noMatch", { query })}
              </p>
            )}
          </div>

          <div className="sidebar-footer">
            <div className="row">
              <span className="name">{t("manual.articleCount", { count: articles.length })}</span>
            </div>
          </div>
        </ShellSidebar>
      )}

      <div className="workspace-header" ref={headerRef}>
        <span className="workspace-title">{selected.title}</span>
      </div>

      <div className="workspace-body">
        <article className="article">
          <h1>{selected.title}</h1>
          <div className="article-meta">
            {t(`manual.categoryLabels.${selected.category}`)} · {t("manual.minRead", { minutes: selected.minutes })}
          </div>
          {selected.paragraphs.map((p, i) => (
            <p key={i}>{p}</p>
          ))}
        </article>
      </div>
    </>
  );
}
