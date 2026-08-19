import { useState } from "react";
import AIControlCenter from "./AIControlCenter";
import Chat from "./Chat";
import Onboarding, { hasAcknowledgedGuardrails } from "./Onboarding";
import Settings from "./Settings";
import Skills from "./Skills";
import Usage from "./Usage";
import "./App.css";

type Tab = "chat" | "control-center" | "skills" | "usage" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "control-center", label: "AI Control Center" },
  { id: "skills", label: "Skills" },
  { id: "usage", label: "Usage" },
  { id: "settings", label: "Settings" },
];

function App() {
  const [tab, setTab] = useState<Tab>("chat");
  // Shows automatically on first launch (per Screen Inventory's decided
  // "Onboarding 強制過一遍 Guardrails 摘要"); Settings can also flip this
  // back to true so the summary stays reachable later, since the rules
  // themselves aren't optional but re-reading them should always be.
  const [showOnboarding, setShowOnboarding] = useState(() => !hasAcknowledgedGuardrails());

  function renderTab() {
    switch (tab) {
      case "chat":
        return <Chat />;
      case "control-center":
        return <AIControlCenter />;
      case "skills":
        return <Skills />;
      case "usage":
        return <Usage />;
      case "settings":
        return <Settings onShowGuardrailsSummary={() => setShowOnboarding(true)} />;
    }
  }

  return (
    <div className="app-shell">
      <nav className="app-tabs">
        {TABS.map((t) => (
          <button key={t.id} className={tab === t.id ? "active" : ""} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <div className="app-tab-content">{renderTab()}</div>
      {showOnboarding && <Onboarding onDismiss={() => setShowOnboarding(false)} />}
    </div>
  );
}

export default App;
