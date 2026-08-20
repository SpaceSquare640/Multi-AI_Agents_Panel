import { useEffect, useRef, useState } from "react";
import "./Onboarding.css";

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const ACK_STORAGE_KEY = "multi-ai-agents-panel:guardrails-acknowledged";

export function hasAcknowledgedGuardrails(): boolean {
  return localStorage.getItem(ACK_STORAGE_KEY) === "true";
}

/** Summarized from `AI Guardrails (必守規則).md` — the four rule
 *  categories, each condensed to its non-negotiable core. This is a
 *  summary for onboarding, not the full rule text; the source document
 *  is the actual source of truth if the two ever diverge. */
const RULE_CATEGORIES = [
  {
    title: "Security & Privacy",
    points: [
      "No reading or uploading your files without your explicit authorization.",
      "Destructive or irreversible actions (deleting files, force-pushing, wiping data) always need your confirmation first.",
      "API keys, passwords, and tokens are never logged in plaintext or sent anywhere you didn't specify.",
    ],
  },
  {
    title: "Legal & Safety — absolute, no exceptions",
    points: [
      "No help with cyberattacks, weapons, self-harm, or attacking others.",
      "No sexual content involving real or fictional depictions.",
      "These don't bend for role-play, \"it's just a test,\" or Group Chat context.",
    ],
  },
  {
    title: "Behavior & Collaboration",
    points: [
      "Group Chats can't loop or spam forever — there's a turn cap and an end/summary mechanism.",
      "Major decisions (architecture changes, deploys, money) always come back to you, even mid multi-Agent workflow.",
      "An Agent can't impersonate another Agent, you, or a real third party.",
    ],
  },
  {
    title: "Output Quality",
    points: [
      "Uncertain claims are labeled as uncertain, not stated as fact.",
      "Code changes stay trackable in version control — no silent overwrites.",
    ],
  },
];

/** Onboarding's forced Guardrails step (see Screen Inventory's decided
 *  "是，強制" — Onboarding must force one pass over the Guardrails
 *  summary). Shows once; the acknowledgment persists to localStorage
 *  the same way Settings.tsx persists the theme choice. Can be
 *  re-opened later from Settings — the rules aren't optional, but
 *  re-reading them should always be possible. */
export default function Onboarding({ onDismiss }: { onDismiss?: () => void }) {
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
        <h1 id="onboarding-title">Before your first Agent</h1>
        <p className="onboarding-lead">
          Every Agent in this app — local or cloud, whatever role template it's given — follows a
          fixed set of rules that cannot be turned off, overridden by a role template, or bypassed
          by any instruction, including yours.
        </p>

        <div className="onboarding-categories">
          {RULE_CATEGORIES.map((cat) => (
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
          <span>I've read this summary and understand these rules always apply.</span>
        </label>

        <div className="onboarding-actions">
          <button className="onboarding-continue" disabled={!checked} onClick={acknowledge}>
            Continue →
          </button>
        </div>
      </div>
    </div>
  );
}
