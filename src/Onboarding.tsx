import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "./shell/Icons";
import "./styles/screens/onboarding.css";

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const ACK_STORAGE_KEY = "multi-ai-agents-panel:guardrails-acknowledged";

export function hasAcknowledgedGuardrails(): boolean {
  return localStorage.getItem(ACK_STORAGE_KEY) === "true";
}

/** The four rule categories' shape — content itself now lives in
 *  `src/locales/en/translation.json` under `onboarding.categories`
 *  (summarized from `AI Guardrails (必守規則).md`; a summary for
 *  onboarding, not the full rule text — the source document is the
 *  actual source of truth if the two ever diverge), pulled via
 *  `t("onboarding.categories", { returnObjects: true })` below. */
interface RuleCategory {
  title: string;
  points: string[];
}

/** One icon per rule category, in the order the categories are written.
 *  Positional rather than keyed off the title, because the titles are
 *  translated and a lookup by translated string would break in six of
 *  the seven locales. The fallback matters: a category added to the
 *  translation file without one here still renders, with the generic
 *  alert rather than no icon at all. */
const CATEGORY_ICONS: IconName[] = ["key", "shield-alert", "chat", "check"];

/** Onboarding's forced Guardrails step (see Screen Inventory's decided
 *  "是，強制" — Onboarding must force one pass over the Guardrails
 *  summary). Shows once; the acknowledgment persists to localStorage
 *  the same way Settings.tsx persists the theme choice. Can be
 *  re-opened later from Settings — the rules aren't optional, but
 *  re-reading them should always be possible. */
export default function Onboarding({ onDismiss }: { onDismiss?: () => void }) {
  const { t } = useTranslation();
  const ruleCategories = t("onboarding.categories", { returnObjects: true }) as RuleCategory[];
  const [checked, setChecked] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setChecked(false);
  }, []);

  // This modal is a *forced* gate (see Screen Inventory's decided "是，
  // 強制") — a keyboard user must not be able to Tab past it into the
  // app behind it, so it needs a real focus trap, not just visual
  // z-index/backdrop layering. Also moves initial focus into the modal
  // on mount, per WCAG 2.1 AA (a Design Principles decided requirement)
  // rather than leaving focus on whatever was focused before it opened.
  useEffect(() => {
    const modal = modalRef.current;
    if (!modal) return;

    const focusables = () => Array.from(modal.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
    focusables()[0]?.focus();

    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Tab" || !modal) return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  function acknowledge() {
    localStorage.setItem(ACK_STORAGE_KEY, "true");
    onDismiss?.();
  }

  return (
    <div className="onboard">
      <div
        className="onboard-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
        ref={modalRef}
      >
        <div className="onboard-mark">
          <div>
            <div className="onboard-app">{t("shell.appName")}</div>
          </div>
        </div>

        <h1 id="onboarding-title">{t("onboarding.title")}</h1>
        <p>{t("onboarding.lead")}</p>

        {/* The design shows "Step 1 of 3" with progress dots. There is no
            step 2 or 3 — this gate is the whole of onboarding — so the
            indicator is not ported rather than shown pointing at steps
            that do not exist. */}
        <div className="rules">
          {ruleCategories.map((cat, i) => (
            <div className="rule" key={cat.title}>
              <Icon name={CATEGORY_ICONS[i] ?? "alert"} />
              <div className="rule-body">
                <h3>{cat.title}</h3>
                <ul>
                  {cat.points.map((p) => (
                    <li key={p}>{p}</li>
                  ))}
                </ul>
              </div>
            </div>
          ))}
        </div>

        <label className="onboard-confirm">
          <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} />
          <span>{t("onboarding.confirmLabel")}</span>
        </label>

        <div className="onboard-foot">
          <span className="spacer" />
          <button className="btn btn-primary" type="button" disabled={!checked} onClick={acknowledge}>
            {t("onboarding.continue")}
          </button>
        </div>
      </div>
    </div>
  );
}
