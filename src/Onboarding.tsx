import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./Onboarding.css";

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
    <div className="onboarding-backdrop">
      <div
        className="onboarding-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
        ref={modalRef}
      >
        <h1 id="onboarding-title">{t("onboarding.title")}</h1>
        <p className="onboarding-lead">{t("onboarding.lead")}</p>

        <div className="onboarding-categories">
          {ruleCategories.map((cat) => (
            <div className="onboarding-category" key={cat.title}>
              <div className="onboarding-category-title">{cat.title}</div>
              <ul>
                {cat.points.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <label className="onboarding-confirm">
          <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} />
          <span>{t("onboarding.confirmLabel")}</span>
        </label>

        <div className="onboarding-actions">
          <button className="onboarding-continue" disabled={!checked} onClick={acknowledge}>
            {t("onboarding.continue")}
          </button>
        </div>
      </div>
    </div>
  );
}
