import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { UsageSummary } from "./types";
import "./Usage.css";

const SOFT_CAP_STORAGE_KEY = "multi-ai-agents-panel:usage-soft-cap";

/** Parses the soft-cap input field's raw string and decides whether
 *  `totalCalls` has crossed it. Extracted as a pure function (rather
 *  than inline arithmetic in the component) so it's independently
 *  testable — blank/non-numeric/zero/negative input all mean "no cap
 *  set", not "always warn". */
export function isOverSoftCap(totalCalls: number, rawCapInput: string): boolean {
  if (rawCapInput.trim() === "") return false;
  const cap = Number(rawCapInput);
  return Number.isFinite(cap) && cap > 0 && totalCalls >= cap;
}

/** High-level usage dashboard: KPI cards + per-provider breakdown,
 *  aggregated from the same `get_usage_summary` data the AI Control
 *  Center's raw per-key table already shows (that table stays — it's
 *  useful for debugging a specific key; this view is for a glance at
 *  the big picture).
 *
 *  Deliberately shows call counts only, not estimated cost — the
 *  Screen Inventory mockup for this screen included a cost figure, but
 *  there is no real pricing data wired up yet (Usage Tracker's cost
 *  estimation is still open in the Backlog: it needs a decision on
 *  where per-model pricing comes from, and today's `usage_log` doesn't
 *  even record token counts). Showing a fabricated cost number would
 *  be worse than not showing one.
 *
 *  What IS real: a soft call-count budget warning — the actual purpose
 *  Architecture.md gives Usage Tracker ("避免失控燒 API 額度"). Only
 *  cloud calls ever reach `usage_log` (local Ollama has no Key Vault
 *  entry to log against — see `dispatch_one`), so `totalCalls` here is
 *  already cloud-only, which is the number that actually costs money. */
export default function Usage() {
  const { t } = useTranslation();
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [softCapInput, setSoftCapInput] = useState(() => localStorage.getItem(SOFT_CAP_STORAGE_KEY) ?? "");

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      setUsage(await invoke<UsageSummary[]>("get_usage_summary"));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  function updateSoftCap(value: string) {
    setSoftCapInput(value);
    if (value.trim() === "") {
      localStorage.removeItem(SOFT_CAP_STORAGE_KEY);
    } else {
      localStorage.setItem(SOFT_CAP_STORAGE_KEY, value);
    }
  }

  const totalSuccess = usage.reduce((sum, u) => sum + u.successCount, 0);
  const totalFailure = usage.reduce((sum, u) => sum + u.failureCount, 0);
  const totalCalls = totalSuccess + totalFailure;
  const failureRate = totalCalls === 0 ? 0 : (totalFailure / totalCalls) * 100;

  const softCap = softCapInput.trim() === "" ? null : Number(softCapInput);
  const overSoftCap = isOverSoftCap(totalCalls, softCapInput);

  const byProvider = new Map<string, { success: number; failure: number }>();
  for (const u of usage) {
    const entry = byProvider.get(u.provider) ?? { success: 0, failure: 0 };
    entry.success += u.successCount;
    entry.failure += u.failureCount;
    byProvider.set(u.provider, entry);
  }
  const providerRows = [...byProvider.entries()].sort((a, b) => b[1].success + b[1].failure - (a[1].success + a[1].failure));
  const maxProviderTotal = Math.max(1, ...providerRows.map(([, v]) => v.success + v.failure));

  return (
    <div className="usage-screen">
      <div className="usage-head">
        <h1>{t("usage.title")}</h1>
        <button onClick={() => void refresh()} disabled={loading}>
          {loading ? t("usage.refreshing") : t("usage.refresh")}
        </button>
      </div>

      {error && <div className="acc-error">{error}</div>}

      <div className="usage-kpi-row">
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">{t("usage.totalCalls")}</div>
          <div className="usage-kpi-value">{totalCalls.toLocaleString()}</div>
        </div>
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">{t("usage.failedCalls")}</div>
          <div className="usage-kpi-value">{totalFailure.toLocaleString()}</div>
        </div>
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">{t("usage.failureRate")}</div>
          <div className="usage-kpi-value">{failureRate.toFixed(1)}%</div>
        </div>
      </div>

      <div className="usage-budget-row">
        <label htmlFor="usage-soft-cap">{t("usage.softBudget")}</label>
        <input
          id="usage-soft-cap"
          type="number"
          min={1}
          placeholder={t("usage.unset")}
          value={softCapInput}
          onChange={(e) => updateSoftCap(e.target.value)}
        />
        <span className="acc-hint">{t("usage.softBudgetHint")}</span>
      </div>

      {overSoftCap && (
        <div className="usage-budget-warning">
          {t("usage.budgetWarning", { cap: softCap!.toLocaleString(), total: totalCalls.toLocaleString() })}
        </div>
      )}

      <p className="acc-hint">{t("usage.costHint")}</p>

      {!loading && usage.length === 0 && <p className="acc-empty">{t("usage.noneRecorded")}</p>}

      {providerRows.length > 0 && (
        <div className="usage-provider-panel">
          <div className="usage-panel-title">{t("usage.callsByProvider")}</div>
          {providerRows.map(([provider, v]) => {
            const total = v.success + v.failure;
            const pct = (total / maxProviderTotal) * 100;
            return (
              <div className="usage-bar-row" key={provider}>
                <span className="usage-bar-label">{provider}</span>
                <div className="usage-bar-track">
                  <div className="usage-bar-fill" style={{ width: `${pct}%` }} />
                </div>
                <span className="usage-bar-value">{total.toLocaleString()}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
