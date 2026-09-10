import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { UsageSummary } from "../types";
import { Icon } from "./Icons";

/** How often the local runtime's reachability is re-checked.
 *
 *  Ollama can be started or stopped from outside this app at any time, so
 *  a value read once at launch goes stale silently — and a status light
 *  that is quietly wrong is worse than no status light. Thirty seconds is
 *  slow enough to be invisible in cost (one HTTP request to /api/tags on
 *  loopback) and fast enough that the indicator is not meaningfully
 *  behind what the user just did in another window. */
const OLLAMA_POLL_MS = 30_000;

/** The persistent bottom bar.
 *
 *  Everything here is read from the running system. The design mockup's
 *  status bar also shows an agent count and a "Today" spend figure; both
 *  are omitted rather than approximated, because the numbers this app can
 *  actually produce today do not mean what those labels claim:
 *
 *  - There is no per-day cost breakdown. `get_usage_summary_with_cost`
 *    returns a lifetime total, and only for OpenRouter keys where both
 *    token counts and model pricing are known. Labelling that "Today"
 *    would be a false statement about the user's spending, so it is
 *    labelled as the estimated total it is, and hidden entirely when
 *    nothing is known.
 *  - The number of running agents lives in Chat's component state, not
 *    anywhere the shell can read. Wiring it up properly belongs with the
 *    Chat port, not with a placeholder here.
 *
 *  Guardrails state is likewise absent for now: the design calls for a
 *  permanent indicator precisely because a rule that silently stops
 *  applying is worse than no rule, and an indicator that is hardcoded to
 *  "enforced" would be that same failure wearing a badge. It arrives when
 *  it can be read from the guardrail configuration rather than asserted. */
export default function StatusBar() {
  const { t, i18n } = useTranslation();
  const [ollamaRunning, setOllamaRunning] = useState<boolean | null>(null);
  const [estimatedCost, setEstimatedCost] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function check() {
      try {
        const running = await invoke<boolean>("ollama_is_running");
        if (!cancelled) setOllamaRunning(running);
      } catch {
        // A failed check is not the same as "not running", but from the
        // user's side both mean the local runtime is not usable right
        // now, and the bar has one dot to say it with.
        if (!cancelled) setOllamaRunning(false);
      }
    }

    void check();
    const timer = setInterval(() => void check(), OLLAMA_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    invoke<UsageSummary[]>("get_usage_summary_with_cost")
      .then((rows) => {
        if (cancelled) return;
        const known = rows
          .map((r) => r.totalEstimatedCostUsd)
          .filter((c): c is number => c !== null);
        setEstimatedCost(known.length > 0 ? known.reduce((a, b) => a + b, 0) : null);
      })
      .catch(() => {
        // Usage is supplementary. If it cannot be read, the bar shows one
        // fewer item rather than an error the user cannot act on.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <footer className="statusbar">
      <span className="statusbar-item">
        <Icon name="local" size="sm" />
        {t("shell.status.localRuntime")}
        <span
          className="status-dot"
          /* "idle", not "off": the design system defines ok / warn / error
             / idle / running and nothing else, so an invented state name
             matches no rule and paints an invisible dot — a status light
             that goes blank exactly when it has something to report. Idle
             (faint grey) is also the honest reading: a local runtime that
             is not running is not an error, it is simply not in use. The
             checking and not-running cases share the dot and are told
             apart by the text beside it. */
          data-state={ollamaRunning ? "ok" : "idle"}
          aria-hidden="true"
        />
        <span className="sr-only">
          {ollamaRunning === null
            ? t("shell.status.checking")
            : ollamaRunning
              ? t("shell.status.running")
              : t("shell.status.notRunning")}
        </span>
      </span>

      <span className="statusbar-spacer" />

      {estimatedCost !== null && (
        <span className="statusbar-item">
          {t("shell.status.estimatedTotal")} <span className="num">${estimatedCost.toFixed(2)}</span>
        </span>
      )}
      <span className="statusbar-item">{i18n.language.toUpperCase()}</span>
    </footer>
  );
}
