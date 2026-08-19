import { useState } from "react";
import AIControlCenter from "./AIControlCenter";
import Chat from "./Chat";
import Settings from "./Settings";
import "./App.css";

type Tab = "chat" | "control-center" | "settings";

function App() {
  const [tab, setTab] = useState<Tab>("chat");

  return (
    <div className="app-shell">
      <nav className="app-tabs">
        <button className={tab === "chat" ? "active" : ""} onClick={() => setTab("chat")}>
          Chat
        </button>
        <button
          className={tab === "control-center" ? "active" : ""}
          onClick={() => setTab("control-center")}
        >
          AI Control Center
        </button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}>
          Settings
        </button>
      </nav>
      <div className="app-tab-content">
        {tab === "chat" ? <Chat /> : tab === "control-center" ? <AIControlCenter /> : <Settings />}
      </div>
    </div>
  );
}

export default App;
