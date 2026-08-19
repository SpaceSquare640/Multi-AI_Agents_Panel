import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyStoredTheme } from "./Settings";

// Apply the user's saved theme choice before first paint, not just after
// Settings mounts — otherwise every launch flashes the OS-default theme
// for a frame even when the user explicitly picked the other one.
applyStoredTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
