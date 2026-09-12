import { useState } from "react";
import AIControlCenter from "./AIControlCenter";
import Chat from "./Chat";
import MachineLearning from "./MachineLearning";
import Manual from "./Manual";
import Notes from "./Notes";
import Onboarding, { hasAcknowledgedGuardrails } from "./Onboarding";
import Settings from "./Settings";
import Skills from "./Skills";
import Usage from "./Usage";
import AppShell, { type TabId } from "./shell/AppShell";
import "./App.css";

type Tab = TabId;

function App() {
  const [tab, setTab] = useState<Tab>("chat");
  // Shows automatically on first launch (per Screen Inventory's decided
  // "Onboarding 強制過一遍 Guardrails 摘要"); Settings can also flip this
  // back to true so the summary stays reachable later, since the rules
  // themselves aren't optional but re-reading them should always be.
  const [showOnboarding, setShowOnboarding] = useState(() => !hasAcknowledgedGuardrails());

  return (
    /* The v2 shell replaces v1's outer chrome — the flat tab strip and the
     * CRT scanline overlay that `.app-shell::before` painted over the whole
     * viewport. Both belonged to the frame, and the frame is what this step
     * ports. The screens themselves are untouched: each still renders from
     * its own stylesheet, inside .workspace. */
    <AppShell active={tab} onNavigate={setTab}>
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
      <div className="app-tab-content" data-screen-pane hidden={tab !== "chat"}>
        <Chat />
      </div>
      <div className="app-tab-content" data-screen-pane hidden={tab !== "control-center"}>
        <AIControlCenter onOpenUsage={() => setTab("usage")} onOpenManual={() => setTab("manual")} />
      </div>
      <div className="app-tab-content" data-screen-pane hidden={tab !== "ml"}>
        <MachineLearning />
      </div>
      <div className="app-tab-content" data-screen-pane hidden={tab !== "skills"}>
        <Skills onOpenChat={() => setTab("chat")} />
      </div>
      {/* screen-v2 marks a screen that has been rebuilt against the v2
          design: it supplies its own workspace header and scrolls its body
          rather than its whole self. The class goes away once every screen
          is ported and the v1 wrapper behaviour is no longer the default. */}
      <div className="app-tab-content screen-v2" data-screen-pane hidden={tab !== "usage"}>
        <Usage />
      </div>
      <div className="app-tab-content" data-screen-pane hidden={tab !== "notes"}>
        <Notes />
      </div>
      <div className="app-tab-content screen-v2" data-screen-pane hidden={tab !== "manual"}>
        <Manual />
      </div>
      <div className="app-tab-content" data-screen-pane hidden={tab !== "settings"}>
        <Settings
          onShowGuardrailsSummary={() => setShowOnboarding(true)}
          onOpenManual={() => setTab("manual")}
        />
      </div>
      {showOnboarding && <Onboarding onDismiss={() => setShowOnboarding(false)} />}
    </AppShell>
  );
}

export default App;
