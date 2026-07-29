import { useState } from "react";
import AIControlCenter from "./AIControlCenter";
import Chat from "./Chat";
import "./App.css";

type Tab = "chat" | "control-center";

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
      </nav>
      <div className="app-tab-content">{tab === "chat" ? <Chat /> : <AIControlCenter />}</div>
    </div>
  );
}

export default App;
