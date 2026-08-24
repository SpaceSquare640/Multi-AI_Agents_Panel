import { useState } from "react";
import AIControlCenter from "./AIControlCenter";
import Chat from "./Chat";
import GameAgent from "./GameAgent";
import Manual from "./Manual";
import Notes from "./Notes";
import Onboarding, { hasAcknowledgedGuardrails } from "./Onboarding";
import SemanticSearch from "./SemanticSearch";
import Settings from "./Settings";
import Skills from "./Skills";
import Usage from "./Usage";
import "./App.css";

type Tab =
  | "chat"
  | "control-center"
  | "semantic-search"
  | "skills"
  | "game-agent"
  | "usage"
  | "notes"
  | "manual"
  | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "control-center", label: "Models" },
  { id: "semantic-search", label: "Semantic Search" },
  { id: "skills", label: "Skills" },
  { id: "game-agent", label: "Game Agent" },
  { id: "usage", label: "Usage" },
  { id: "notes", label: "Notes" },
  { id: "manual", label: "Help" },
  { id: "settings", label: "Settings" },
];

function App() {
  const [tab, setTab] = useState<Tab>("chat");
  // Shows automatically on first launch (per Screen Inventory's decided
  // "Onboarding 強制過一遍 Guardrails 摘要"); Settings can also flip this
  // back to true so the summary stays reachable later, since the rules
  // themselves aren't optional but re-reading them should always be.
  const [showOnboarding, setShowOnboarding] = useState(() => !hasAcknowledgedGuardrails());

  return (
    <div className="app-shell">
      <nav className="app-tabs">
        {TABS.map((t) => (
          <button key={t.id} className={tab === t.id ? "active" : ""} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      {/* Every page stays mounted the whole time the app is open — only
       *  `hidden` toggles, never a conditional-render swap. Found via
       *  code inspection (not a guess): the old `renderTab()` switch
       *  returned exactly one page component, so React fully unmounted
       *  whichever page you left and mounted the next one from scratch
       *  on every single tab click. That meant every switch paid to
       *  rebuild the whole component tree AND re-fired every mount
       *  effect as a fresh round of Tauri IPC calls (Chat re-fetches
       *  agents/sessions/role templates; AI Control Center re-fetches
       *  keys/usage/Ollama status; Skills re-lists skills — all of it,
       *  every time, even switching back to a tab you'd just left).
       *  Combined with a slow in-flight call from the page you're
       *  leaving (e.g. Agent function calling's multi-round Anthropic
       *  conversation — see the mutex fix this session already
       *  shipped), the freshly-mounted page's own new IPC calls would
       *  queue up behind whatever backend lock the old page's abandoned
       *  call was still holding — this is what "switching pages
       *  sometimes causes no response" actually was. `hidden` keeps
       *  every page's state (and its one-time mount effects) alive
       *  permanently instead: switching tabs becomes a plain CSS
       *  visibility toggle, no remount, no re-fetch, no chance of a
       *  freshly-mounted page's IPC call getting stuck behind a
       *  previous page's still-in-flight one. */}
      <div className="app-tab-content" hidden={tab !== "chat"}>
        <Chat />
      </div>
      <div className="app-tab-content" hidden={tab !== "control-center"}>
        <AIControlCenter onOpenUsage={() => setTab("usage")} onOpenManual={() => setTab("manual")} />
      </div>
      <div className="app-tab-content" hidden={tab !== "semantic-search"}>
        <SemanticSearch />
      </div>
      <div className="app-tab-content" hidden={tab !== "skills"}>
        <Skills onOpenChat={() => setTab("chat")} />
      </div>
      <div className="app-tab-content" hidden={tab !== "game-agent"}>
        <GameAgent />
      </div>
      <div className="app-tab-content" hidden={tab !== "usage"}>
        <Usage />
      </div>
      <div className="app-tab-content" hidden={tab !== "notes"}>
        <Notes />
      </div>
      <div className="app-tab-content" hidden={tab !== "manual"}>
        <Manual />
      </div>
      <div className="app-tab-content" hidden={tab !== "settings"}>
        <Settings
          onShowGuardrailsSummary={() => setShowOnboarding(true)}
          onOpenManual={() => setTab("manual")}
        />
      </div>
      {showOnboarding && <Onboarding onDismiss={() => setShowOnboarding(false)} />}
    </div>
  );
}

export default App;
