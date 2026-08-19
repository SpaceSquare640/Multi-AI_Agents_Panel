import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UsageSummary } from "./types";
import "./Usage.css";

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
 *  be worse than not showing one. */
export default function Usage() {
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  const totalSuccess = usage.reduce((sum, u) => sum + u.successCount, 0);
  const totalFailure = usage.reduce((sum, u) => sum + u.failureCount, 0);
  const totalCalls = totalSuccess + totalFailure;
  const failureRate = totalCalls === 0 ? 0 : (totalFailure / totalCalls) * 100;

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
