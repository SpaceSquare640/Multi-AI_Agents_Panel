import { useMemo, useState } from "react";
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
 *  rule (what counts as a hit) is testable independent of React state. */
export function filterArticles(articles: Article[], query: string): Article[] {
  const q = query.trim().toLowerCase();
  if (!q) return articles;
  return articles.filter(
    (a) => a.title.toLowerCase().includes(q) || a.paragraphs.some((p) => p.toLowerCase().includes(q)),
  );
}

/** The in-app User Manual's initial content — per Design Principles'
 *  decided scope: "獨立 Session / Group Chat 差異、如何設定本地模型、
 *  如何設定雲端 API Key、角色模板如何使用、Guardrails 是什麼". Real
 *  descriptions of what this app's already-shipped features actually
 *  do, not placeholder text — each one describes a screen/flow that
 *  exists in Chat.tsx, AIControlCenter.tsx, or Onboarding.tsx today. */
const ARTICLES: Article[] = [
  {
    id: "independent-vs-group",
    title: "Independent Sessions vs Group Chat",
    category: "Getting Started",
    minutes: 2,
    paragraphs: [
      "An Independent Session is one Agent, one conversation — like a normal chat window. Use it when you just need one model working on one task.",
      "A Group Chat has multiple Agents talking in the same session, in a single timeline, taking turns round-robin in the order they joined. You can type @AgentName to let a specific Agent jump in out of turn without disrupting whose turn is next.",
      "Group Chats have a loop safety-net: after several consecutive Agent turns with no new message from you, the conversation pauses so you always stay in control. \"Let them continue\" advances one more turn manually; \"End meeting\" asks a summarizer Agent to wrap up the discussion.",
    ],
  },
  {
    id: "local-vs-cloud",
    title: "Setting up a local model (Ollama)",
    category: "Getting Started",
    minutes: 3,
    paragraphs: [
      "Local Agents run entirely on your own device through Ollama — nothing you send them leaves your machine, and there's no per-call cost.",
      "Install Ollama itself first (this app doesn't bundle it). Once it's running, the AI Control Center's \"Local AI Models (Ollama)\" section shows what's installed and lets you pull new models by name — no terminal needed.",
      "When creating an Agent, pick \"local\" as its provider kind and choose an installed model. Larger models need more RAM/VRAM; if a model is too large for your hardware it will simply run very slowly or fail to load.",
    ],
  },
  {
    id: "cloud-api-keys",
    title: "Adding a cloud API key",
    category: "Getting Started",
    minutes: 2,
    paragraphs: [
      "Cloud Agents call a provider — Anthropic, OpenAI, or OpenRouter — using an API key you provide yourself. This app never provides its own shared keys or bills you directly.",
      "Add a key under the AI Control Center's \"Cloud AI Models\" section. Keys are stored locally through your OS credential store, never sent anywhere except the provider you're calling.",
      "You can add multiple keys per provider and optionally pin a specific Agent to a specific key. Without a pin, the most-recently-added key for that provider is tried first, falling back to older ones if a call fails.",
    ],
  },
  {
    id: "role-templates",
    title: "Using Role Templates",
    category: "Agents",
    minutes: 2,
    paragraphs: [
      "A Role Template is a pre-written system prompt that gives an Agent a specific job — Product Lead, Lead Architect, Full-Stack Developer, and others modeled on a small one-person company.",
      "Default templates ship with the app and reset to the latest version on every update — don't edit them directly. To customize one, duplicate it into User Custom first; anything in User Custom is yours permanently and survives app updates.",
      "Each template can suggest a provider kind (e.g. \"Full-Stack Developer\" suggests cloud, \"Issue Manager\" suggests local) — this is only a default suggestion when creating an Agent, never a hard requirement.",
    ],
  },
  {
    id: "guardrails",
    title: "What Guardrails are",
    category: "Safety",
    minutes: 2,
    paragraphs: [
      "Guardrails are a fixed set of rules every Agent follows — local or cloud, whatever role template it's using. They can't be turned off, overridden by a role template, or bypassed by any instruction, including yours.",
      "They cover things like: never reading your files without explicit authorization, always confirming before an irreversible action, refusing to help with attacks or illegal activity, and not looping forever in a Group Chat.",
      "You saw a summary of these on first launch (Settings → Safety → \"View summary again\" if you want to re-read it). If an Agent refuses part of a request, it's because of one of these rules — it will tell you which one.",
    ],
  },
];

const CATEGORY_ORDER = ["Getting Started", "Agents", "Safety"];

export default function Manual() {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(ARTICLES[0].id);

  const filtered = useMemo(() => filterArticles(ARTICLES, query), [query]);

  const selected = ARTICLES.find((a) => a.id === selectedId) ?? filtered[0] ?? ARTICLES[0];

  return (
    <div className="manual-screen">
      <aside className="manual-toc">
        <input
          className="manual-search"
          type="text"
          placeholder="Search the manual…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {CATEGORY_ORDER.map((category) => {
          const inCategory = filtered.filter((a) => a.category === category);
          if (inCategory.length === 0) return null;
          return (
            <div key={category}>
              <div className="manual-toc-section">{category}</div>
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
        {filtered.length === 0 && <p className="acc-empty">No articles match "{query}".</p>}
      </aside>

      <div className="manual-content">
        <h1>{selected.title}</h1>
        <div className="manual-meta">
          {selected.category} · {selected.minutes} min read
        </div>
        {selected.paragraphs.map((p, i) => (
          <p key={i}>{p}</p>
        ))}
      </div>
    </div>
  );
}
