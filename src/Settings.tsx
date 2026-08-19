import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./Settings.css";

export type ThemeChoice = "system" | "dark" | "light";

const THEME_STORAGE_KEY = "multi-ai-agents-panel:theme";

/** Reads the persisted theme choice and applies it to the document root —
 *  called once at startup (see main.tsx) so the correct theme is in place
 *  before first paint, not just after Settings mounts. Exported so
 *  main.tsx can call it without importing the whole Settings screen. */
export function applyStoredTheme(): ThemeChoice {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  const choice: ThemeChoice = stored === "dark" || stored === "light" ? stored : "system";
  applyTheme(choice);
  return choice;
}

function applyTheme(choice: ThemeChoice) {
  if (choice === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = choice;
  }
  // Best-effort: also sync the native window chrome (title bar) so it
  // doesn't visually clash with an explicit in-app override. Not
  // supported on every platform/window manager — a failure here just
  // leaves the OS default title bar, which is a harmless fallback.
  // getCurrentWindow() itself throws synchronously (not just a rejected
  // promise) outside a real Tauri webview — e.g. this app's plain
  // browser dev-preview — so this whole call must be try/caught, not
  // just the promise.
  try {
    getCurrentWindow()
      .setTheme(choice === "system" ? null : choice)
      .catch(() => {});
  } catch {
    // No Tauri IPC bridge available (browser preview) — CSS theme still applied above.
  }
}

/** Language options per Design Principles — English is the source of
 *  truth and the only one with real strings today. The rest are listed
 *  so users can see what's planned, not because they work yet: picking
 *  one would silently show untranslated English UI, which is worse than
 *  not offering the choice at all. */
const LANGUAGES = [
  { code: "en", label: "English", ready: true },
  { code: "zh-Hant", label: "繁體中文", ready: false },
  { code: "zh-Hans", label: "简体中文", ready: false },
  { code: "fr", label: "Français", ready: false },
  { code: "de", label: "Deutsch", ready: false },
  { code: "ja", label: "日本語", ready: false },
  { code: "ko", label: "한국어", ready: false },
];

export default function Settings({ onShowGuardrailsSummary }: { onShowGuardrailsSummary?: () => void }) {
  const [theme, setThemeState] = useState<ThemeChoice>("system");
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    setThemeState(applyStoredTheme());
    try {
      getVersion()
        .then(setVersion)
        .catch(() => setVersion(null));
    } catch {
      // No Tauri IPC bridge available (browser preview).
      setVersion(null);
    }
  }, []);

  function chooseTheme(choice: ThemeChoice) {
    localStorage.setItem(THEME_STORAGE_KEY, choice);
    applyTheme(choice);
    setThemeState(choice);
  }

  return (
    <div className="settings-screen">
      <section className="acc-section">
        <h2>Appearance</h2>
        <div className="settings-row">
          <div>
            <div className="settings-row-label">Theme</div>
            <div className="acc-hint">Dark is the default aesthetic; Light is fully supported too.</div>
          </div>
          <div className="theme-toggle">
            {(["system", "dark", "light"] as const).map((choice) => (
              <button
                key={choice}
                className={theme === choice ? "theme-opt active" : "theme-opt"}
                onClick={() => chooseTheme(choice)}
              >
                {choice === "system" ? "System" : choice === "dark" ? "Dark" : "Light"}
              </button>
            ))}
          </div>
        </div>
      </section>

      <section className="acc-section">
        <h2>Language</h2>
        <div className="settings-row">
          <div>
            <div className="settings-row-label">Interface language</div>
            <div className="acc-hint">
              Default: English. Missing translations fall back to English — see{" "}
              <a
                href="https://github.com/SpaceSquare640/Multi-AI_Agents_Panel"
                target="_blank"
                rel="noreferrer"
              >
                CONTRIBUTING
              </a>{" "}
              to help translate.
            </div>
          </div>
        </div>
        <ul className="acc-model-list">
          {LANGUAGES.map((lang) => (
            <li key={lang.code}>
              <span>{lang.label}</span>
              {lang.ready ? (
                <span className="lang-tag active">selected</span>
              ) : (
                <span className="lang-tag">coming soon</span>
              )}
            </li>
          ))}
        </ul>
      </section>

      <section className="acc-section">
        <h2>Safety</h2>
        <div className="settings-row">
          <div>
            <div className="settings-row-label">AI Guardrails</div>
            <div className="acc-hint">
              The non-negotiable rules every Agent always follows — shown once at first launch.
            </div>
          </div>
          <button onClick={() => onShowGuardrailsSummary?.()}>View summary again</button>
        </div>
      </section>

      <section className="acc-section">
        <h2>About</h2>
        <div className="settings-row">
          <span className="settings-row-label">Version</span>
          <span className="acc-mono">{version ?? "—"}</span>
        </div>
      </section>
    </div>
  );
}
