import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import i18n, { LANGUAGE_STORAGE_KEY } from "./i18n";

type UpdateCheckResult = {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
};

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
 *  truth; all seven now have real translation files (see i18n.ts). */
const LANGUAGES = [
  { code: "en", label: "English" },
  { code: "zh-Hant", label: "繁體中文" },
  { code: "zh-Hans", label: "简体中文" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
];

/** Settings, rebuilt against the v2 design.
 *
 *  Four of the design's rows are not here, because the app has nothing
 *  behind them: text size and reduce-motion (no such preference exists),
 *  the whole Data section (data folder location, on-disk sizes, backups
 *  and erase-everything — none of it has a command), and the guardrails
 *  "blocked this month" count (blocks are not tallied anywhere). The
 *  design's own Guardrails note — that the section deliberately offers
 *  no switches, because a rule that could be turned off from a settings
 *  screen is not a rule — is kept, since that part is true here too. */
export default function Settings({
  onShowGuardrailsSummary,
  onOpenManual,
}: {
  onShowGuardrailsSummary?: () => void;
  onOpenManual?: () => void;
}) {
  const { t } = useTranslation();
  const [theme, setThemeState] = useState<ThemeChoice>("system");
  const [version, setVersion] = useState<string | null>(null);
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult | null>(null);
  const [updateCheckError, setUpdateCheckError] = useState<string | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [customInstructions, setCustomInstructions] = useState("");
  const [savedCustomInstructions, setSavedCustomInstructions] = useState("");
  const [savingInstructions, setSavingInstructions] = useState(false);
  const [instructionsSaved, setInstructionsSaved] = useState(false);
  const [instructionsError, setInstructionsError] = useState<string | null>(null);

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
    invoke<string | null>("get_custom_instructions")
      .then((value) => {
        setCustomInstructions(value ?? "");
        setSavedCustomInstructions(value ?? "");
      })
      .catch(() => {
        // No Tauri IPC bridge available (browser preview) — leave blank.
      });
  }, []);

  async function saveCustomInstructions() {
    setSavingInstructions(true);
    setInstructionsSaved(false);
    setInstructionsError(null);
    try {
      await invoke("set_custom_instructions", { content: customInstructions });
      setSavedCustomInstructions(customInstructions);
      setInstructionsSaved(true);
    } catch (err) {
      setInstructionsError(String(err));
    } finally {
      setSavingInstructions(false);
    }
  }

  function chooseTheme(choice: ThemeChoice) {
    localStorage.setItem(THEME_STORAGE_KEY, choice);
    applyTheme(choice);
    setThemeState(choice);
  }

  function chooseLanguage(code: string) {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, code);
    void i18n.changeLanguage(code);
  }

  async function checkForUpdate() {
    setCheckingUpdate(true);
    setUpdateCheckError(null);
    try {
      const result = await invoke<UpdateCheckResult>("check_for_update", {
        currentVersion: version ?? (await getVersion()),
      });
      setUpdateCheck(result);
    } catch (err) {
      setUpdateCheckError(String(err));
      setUpdateCheck(null);
    } finally {
      setCheckingUpdate(false);
    }
  }

  const instructionsStatus = instructionsSaved
    ? t("settings.customInstructions.saved")
    : customInstructions !== savedCustomInstructions
      ? t("settings.customInstructions.unsaved")
      : null;

  return (
    <>
      <div className="workspace-header">
        <span className="workspace-title">{t("settings.title")}</span>
      </div>

      <div className="workspace-body">
        <div className="pane">
          <section>
            <div className="section-head">
              <h2>{t("settings.appearance.heading")}</h2>
            </div>
            <div className="card settings-group">
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.appearance.theme")}</div>
                  <div className="d">{t("settings.appearance.themeHint")}</div>
                </div>
                <div className="settings-control">
                  <div className="segmented" role="group" aria-label={t("settings.appearance.theme")}>
                    {(["system", "light", "dark"] as const).map((choice) => (
                      <button
                        key={choice}
                        type="button"
                        aria-pressed={theme === choice}
                        onClick={() => chooseTheme(choice)}
                      >
                        {choice === "system"
                          ? t("settings.appearance.themeSystem")
                          : choice === "dark"
                            ? t("settings.appearance.themeDark")
                            : t("settings.appearance.themeLight")}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </section>

          <section>
            <div className="section-head">
              <h2>{t("settings.language.heading")}</h2>
            </div>
            <div className="card settings-group">
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.language.interfaceLanguage")}</div>
                  <div className="d">
                    {t("settings.language.hintBeforeLink")}{" "}
                    <a href="https://github.com/SpaceSquare640/Multi-AI_Agents_Panel" target="_blank" rel="noreferrer">
                      {t("settings.language.contributingLink")}
                    </a>{" "}
                    {t("settings.language.hintAfterLink")}
                  </div>
                </div>
                <div className="settings-control">
                  {/* A select rather than the old list of seven rows: the
                      design uses one, and seven languages is past the point
                      where a row each earns its vertical space. */}
                  <select
                    className="select"
                    aria-label={t("settings.language.interfaceLanguage")}
                    value={i18n.resolvedLanguage}
                    onChange={(e) => chooseLanguage(e.target.value)}
                  >
                    {LANGUAGES.map((lang) => (
                      <option key={lang.code} value={lang.code}>
                        {lang.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>
          </section>

          <section>
            <div className="section-head">
              <h2>{t("settings.customInstructions.heading")}</h2>
            </div>
            <div className="card card-pad">
              <p className="pane-intro">{t("settings.customInstructions.hint")}</p>
              <textarea
                className="textarea settings-instructions"
                value={customInstructions}
                onChange={(e) => {
                  setCustomInstructions(e.target.value);
                  setInstructionsSaved(false);
                }}
                placeholder={t("settings.customInstructions.placeholder")}
                rows={6}
              />
              {instructionsError && (
                <div className="callout" data-kind="danger">
                  <div className="callout-body">{instructionsError}</div>
                </div>
              )}
              <div className="settings-actions">
                <span className="settings-status">{instructionsStatus}</span>
                <button
                  className="btn btn-primary btn-sm"
                  type="button"
                  onClick={() => void saveCustomInstructions()}
                  disabled={savingInstructions || customInstructions === savedCustomInstructions}
                >
                  {savingInstructions ? t("settings.customInstructions.saving") : t("settings.customInstructions.save")}
                </button>
              </div>
            </div>
          </section>

          <section>
            <div className="section-head">
              <h2>{t("settings.safety.heading")}</h2>
            </div>
            <p className="pane-intro">{t("settings.safety.noSwitches")}</p>
            <div className="card settings-group">
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.safety.guardrails")}</div>
                  <div className="d">{t("settings.safety.guardrailsHint")}</div>
                </div>
                <div className="settings-control">
                  <button className="btn btn-secondary btn-sm" type="button" onClick={() => onShowGuardrailsSummary?.()}>
                    {t("settings.safety.viewSummaryAgain")}
                  </button>
                </div>
              </div>
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.safety.llamaGuardTitle")}</div>
                  <div className="d">{t("settings.safety.llamaGuardHint")}</div>
                </div>
              </div>
            </div>
          </section>

          <section>
            <div className="section-head">
              <h2>{t("settings.about.heading")}</h2>
            </div>
            <div className="card settings-group">
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.about.version")}</div>
                </div>
                <div className="settings-control">
                  <span className="mono">{version ?? "—"}</span>
                </div>
              </div>
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.about.userManual")}</div>
                </div>
                <div className="settings-control">
                  <button className="btn btn-secondary btn-sm" type="button" onClick={() => onOpenManual?.()}>
                    {t("settings.about.open")}
                  </button>
                </div>
              </div>
              <div className="settings-row">
                <div className="settings-label">
                  <div className="t">{t("settings.about.checkForUpdates")}</div>
                  {updateCheckError && <div className="d">{t("settings.about.checkFailed", { error: updateCheckError })}</div>}
                  {updateCheck && !updateCheckError && (
                    <div className="d">
                      {updateCheck.updateAvailable
                        ? t("settings.about.updateAvailable", { version: updateCheck.latestVersion })
                        : t("settings.about.upToDate")}
                    </div>
                  )}
                </div>
                <div className="settings-control settings-control-pair">
                  <button className="btn btn-secondary btn-sm" type="button" onClick={checkForUpdate} disabled={checkingUpdate}>
                    {checkingUpdate ? t("settings.about.checking") : t("settings.about.check")}
                  </button>
                  {updateCheck?.updateAvailable && (
                    <button className="btn btn-ghost btn-sm" type="button" onClick={() => openUrl(updateCheck.releaseUrl).catch(() => {})}>
                      {t("settings.about.viewRelease")}
                    </button>
                  )}
                </div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
