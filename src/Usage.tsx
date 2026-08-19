import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UsageSummary } from "./types";
import "./Usage.css";

const SOFT_CAP_STORAGE_KEY = "multi-ai-agents-panel:usage-soft-cap";

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
  const overSoftCap = softCap !== null && Number.isFinite(softCap) && softCap > 0 && totalCalls >= softCap;

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
        <h1>Usage</h1>
        <button onClick={() => void refresh()} disabled={loading}>
          {loading ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {error && <div className="acc-error">{error}</div>}

      <div className="usage-kpi-row">
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">Total calls</div>
          <div className="usage-kpi-value">{totalCalls.toLocaleString()}</div>
        </div>
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">Failed calls</div>
          <div className="usage-kpi-value">{totalFailure.toLocaleString()}</div>
        </div>
        <div className="usage-kpi-card">
          <div className="usage-kpi-label">Failure rate</div>
          <div className="usage-kpi-value">{failureRate.toFixed(1)}%</div>
        </div>
      </div>

      <div className="usage-budget-row">
        <label htmlFor="usage-soft-cap">Soft call budget</label>
        <input
          id="usage-soft-cap"
          type="number"
          min={1}
          placeholder="unset"
          value={softCapInput}
          onChange={(e) => updateSoftCap(e.target.value)}
        />
        <span className="acc-hint">calls — a local reminder only, doesn't block anything</span>
      </div>

      {overSoftCap && (
        <div className="usage-budget-warning">
          ⚠ You've reached your soft budget of {softCap!.toLocaleString()} calls ({totalCalls.toLocaleString()} so
          far). This is only a reminder — cloud Agents keep working.
        </div>
      )}

      <p className="acc-hint">
        Estimated cost isn't shown yet — this app doesn't record token counts or per-model pricing
        today, so there's nothing real to compute it from. See the per-key breakdown below (also on
        the AI Control Center page) for raw success/failure counts.
      </p>

      {!loading && usage.length === 0 && <p className="acc-empty">No usage recorded yet.</p>}

      {providerRows.length > 0 && (
        <div className="usage-provider-panel">
          <div className="usage-panel-title">Calls by provider</div>
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
