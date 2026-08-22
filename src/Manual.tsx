import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import "./Manual.css";

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
 *  displayed, not the original English source. Since English is the
 *  only real locale today, that distinction has no visible effect yet,
 *  but the search logic is already correct for when it does. */
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
const CATEGORY_ORDER = ["gettingStarted", "agents", "safety"];

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

  const filtered = useMemo(() => filterArticles(articles, query), [articles, query]);

  const selected = articles.find((a) => a.id === selectedId) ?? filtered[0] ?? articles[0];

  return (
    <div className="manual-screen">
      <aside className="manual-toc">
        <input
          className="manual-search"
          type="text"
          placeholder={t("manual.searchPlaceholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {CATEGORY_ORDER.map((category) => {
          const inCategory = filtered.filter((a) => a.category === category);
          if (inCategory.length === 0) return null;
          return (
            <div key={category}>
              <div className="manual-toc-section">{t(`manual.categoryLabels.${category}`)}</div>
              {inCategory.map((a) => (
                <button
                  key={a.id}
                  className={a.id === selected.id ? "manual-toc-item active" : "manual-toc-item"}
                  onClick={() => setSelectedId(a.id)}
                >
                  {a.title}
                </button>
              ))}
            </div>
          );
        })}
        {filtered.length === 0 && <p className="acc-empty">{t("manual.noMatch", { query })}</p>}
      </aside>

      <div className="manual-content">
        <h1>{selected.title}</h1>
        <div className="manual-meta">
          {t(`manual.categoryLabels.${selected.category}`)} · {t("manual.minRead", { minutes: selected.minutes })}
        </div>
        {selected.paragraphs.map((p, i) => (
          <p key={i}>{p}</p>
        ))}
      </div>
    </div>
  );
}
