import React from "react";
import ReactDOM from "react-dom/client";
// Imported before App so the token layer is defined before any stylesheet
// that reads it. Vite emits CSS in import order, and a var() that resolves
// before its :root definition is loaded falls back to nothing rather than
// erroring — a failure that shows up as an invisible element, not a build
// break, so the ordering is load-bearing.
//
// The v2 token layer only DEFINES custom properties (plus color-scheme);
// it selects nothing and styles nothing on its own. Loading it alongside
// the v1 stylesheets is therefore inert until a v2 component reads from
// it. See src/styles/tokens.css for the one place the two vocabularies
// would have collided, and what was done about it.
import "./styles/tokens.css";
// The v2 component layer. Ported whole, unlike base.css and screens.css:
// every one of its 181 rules is a class selector, and none of its 86 class
// names appears anywhere in the v1 stylesheets — so it changes nothing that
// is on screen today and only takes effect on markup written against it.
// (screens.css cannot come over this way: its `.settings-row` collides with
// v1's, so it moves a screen at a time, with the rest of that screen.)
import "./styles/components.css";
import App from "./App";
import { applyStoredTheme } from "./Settings";
import "./i18n";

// Apply the user's saved theme choice before first paint, not just after
// Settings mounts — otherwise every launch flashes the OS-default theme
// for a frame even when the user explicitly picked the other one.
applyStoredTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
