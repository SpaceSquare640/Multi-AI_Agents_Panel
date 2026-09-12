import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Agent, Session, UsageSummary } from "./types";
import "./styles/screens/usage.css";

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
 *  aggregated from the same underlying data the AI Control Center's raw
 *  per-key table already shows (that table stays — it's useful for
 *  debugging a specific key; this view is for a glance at the big
 *  picture).
 *
 *  Estimated cost is real, but partial: uses `get_usage_summary_with_cost`
 *  (not the plain `get_usage_summary`), which only fills in a number for
 *  OpenRouter keys with recorded token counts and known model pricing
 *  (see `agent_manager::cost`'s module docs) — Anthropic/OpenAI/local
 *  providers contribute `null`, so the total shown is a known-cost
 *  subtotal, not a complete bill. The KPI card says so explicitly rather
 *  than implying completeness it doesn't have.
 *
 *  Also real: a soft call-count budget warning — the actual purpose
 *  Architecture.md gives Usage Tracker ("避免失控燒 API 額度"). Only
 *  cloud calls ever reach `usage_log` (local Ollama has no Key Vault
 *  entry to log against — see `dispatch_one`), so `totalCalls` here is
 *  already cloud-only, which is the number that actually costs money.
 *
 *  First screen rebuilt against the v2 design. Two places where it
 *  deliberately departs from the mockup, both for the same reason — the
 *  mockup's figures are illustrative and this app does not have them:
 *
 *  - The mockup's budget is an amount of money with a meter and a hard
 *    cap switch. The budget this app actually has is a count of calls,
 *    and there is no hard cap to switch on. The meter and the statement
 *    of what happens at the limit come over; the currency and the switch
 *    do not.
 *  - "Spent this month" / "Today" / the token split need per-period and
 *    per-token records that `usage_log` does not keep. The four figures
 *    that exist are shown instead, rather than four boxes where two are
 *    invented. */
export default function Usage() {
  const { t } = useTranslation();
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
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
      const [usageResult, agentsResult, sessionsResult] = await Promise.all([
        invoke<UsageSummary[]>("get_usage_summary_with_cost"),
        invoke<Agent[]>("list_agents"),
        invoke<Session[]>("list_sessions"),
      ]);
      setUsage(usageResult);
      setAgents(agentsResult);
      setSessions(sessionsResult);
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

  const keysWithKnownCost = usage.filter((u) => u.totalEstimatedCostUsd !== null);
  const totalEstimatedCostUsd =
    keysWithKnownCost.length === 0 ? null : keysWithKnownCost.reduce((sum, u) => sum + (u.totalEstimatedCostUsd ?? 0), 0);

  const softCap = softCapInput.trim() === "" ? null : Number(softCapInput);
  const overSoftCap = isOverSoftCap(totalCalls, softCapInput);
  /* Clamped so the fill never runs past its track once the cap is passed;
     the number beside it still reads past 100%, which is the honest thing
     for the figure to do even when the bar cannot. */
  const budgetPct = softCap && softCap > 0 ? Math.min(100, (totalCalls / softCap) * 100) : 0;

  const byProvider = new Map<string, { success: number; failure: number }>();
  for (const u of usage) {
    const entry = byProvider.get(u.provider) ?? { success: 0, failure: 0 };
    entry.success += u.successCount;
    entry.failure += u.failureCount;
    byProvider.set(u.provider, entry);
  }
  const providerRows = [...byProvider.entries()].sort((a, b) => b[1].success + b[1].failure - (a[1].success + a[1].failure));
  const maxProviderTotal = Math.max(1, ...providerRows.map(([, v]) => v.success + v.failure));

  const localAgents = agents.filter((a) => a.providerKind === "local").length;
  const cloudAgents = agents.length - localAgents;
  const independentSessions = sessions.filter((s) => s.kind === "independent").length;
  const groupChats = sessions.length - independentSessions;
  const distinctProviders = byProvider.size;

  return (
    <>
      <div className="workspace-header">
        <span className="workspace-title">{t("usage.title")}</span>
        <div className="workspace-actions">
          <button className="btn btn-ghost btn-sm" type="button" onClick={() => void refresh()} disabled={loading}>
            {loading ? t("usage.refreshing") : t("usage.refresh")}
          </button>
        </div>
      </div>

      <div className="workspace-body">
        <div className="pane pane-wide">
          {error && (
            <div className="callout" data-kind="danger">
              <div className="callout-body">{error}</div>
            </div>
          )}

          <p className="pane-intro">{t("usage.costHint")}</p>

          <div className="stat-row">
            <div className="stat">
              <div className="k">{t("usage.totalCalls")}</div>
              <div className="v">{totalCalls.toLocaleString()}</div>
            </div>
            <div className="stat">
              <div className="k">{t("usage.failedCalls")}</div>
              <div className="v">{totalFailure.toLocaleString()}</div>
            </div>
            <div className="stat">
              <div className="k">{t("usage.failureRate")}</div>
              <div className="v">{failureRate.toFixed(1)}%</div>
            </div>
            <div className="stat">
              <div className="k">{t("usage.estimatedCost")}</div>
              <div className="v">{totalEstimatedCostUsd === null ? "—" : `$${totalEstimatedCostUsd.toFixed(4)}`}</div>
              <div className="d">{t("usage.estimatedCostHint")}</div>
            </div>
          </div>

          <section>
            <div className="section-head">
              <h2>{t("usage.budget")}</h2>
            </div>

            <div className="card budget">
              <div className="budget-head">
                <h3>{t("usage.warningThreshold")}</h3>
                <span className="v">
                  {totalCalls.toLocaleString()} / {softCap === null ? t("usage.budgetNotSet") : softCap.toLocaleString()}
                </span>
              </div>

              {/* Rendered only when a cap exists: a meter with nothing to
                  measure against would be a bar permanently at zero, which
                  reads as "none used" rather than "no limit set". */}
              {softCap !== null && (
                <>
                  <div
                    className="meter"
                    data-state={overSoftCap ? "over" : budgetPct >= 80 ? "warning" : undefined}
                    role="progressbar"
                    aria-valuenow={Math.round(budgetPct)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-label={t("usage.softBudget")}
                  >
                    <div className="meter-fill" style={{ width: `${budgetPct}%` }} />
                  </div>
                  <div className="budget-marks">
                    <span>0</span>
                    <span>{softCap.toLocaleString()}</span>
                  </div>
                </>
              )}

              {overSoftCap && (
                <div className="callout" data-kind="warning" style={{ marginTop: "var(--space-6)" }}>
                  <div className="callout-body">
                    {t("usage.budgetWarning", { cap: softCap!.toLocaleString(), total: totalCalls.toLocaleString() })}
                  </div>
                </div>
              )}

              <div className="settings-row" style={{ paddingInline: 0, marginTop: "var(--space-5)", borderTop: "1px solid var(--border)" }}>
                <div className="settings-label">
                  <div className="t">
                    <label htmlFor="usage-soft-cap">{t("usage.softBudget")}</label>
                  </div>
                  <div className="d">{t("usage.softBudgetHint")}</div>
                </div>
                <div className="settings-control">
                  <input
                    id="usage-soft-cap"
                    className="input"
                    type="number"
                    min={1}
                    placeholder={t("usage.unset")}
                    value={softCapInput}
                    onChange={(e) => updateSoftCap(e.target.value)}
                  />
                </div>
              </div>
            </div>
          </section>

          {!loading && usage.length === 0 && <p className="pane-intro">{t("usage.noneRecorded")}</p>}

          {providerRows.length > 0 && (
            <section>
              <div className="section-head">
                <h2>{t("usage.callsByProvider")}</h2>
                <span className="count">{totalCalls.toLocaleString()}</span>
              </div>
              <div className="card card-pad">
                <div className="bars">
                  {providerRows.map(([provider, v]) => {
                    const total = v.success + v.failure;
                    return (
                      <div className="bar-row" key={provider}>
                        <span className="who">{provider}</span>
                        <div className="bar-track">
                          {/* Always "cloud": only calls made against a Key
                              Vault entry reach usage_log, and local models
                              have none — so nothing local can appear here. */}
                          <div className="bar-fill" data-where="cloud" style={{ width: `${(total / maxProviderTotal) * 100}%` }} />
                        </div>
                        <span className="val">{total.toLocaleString()}</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </section>
          )}

          <section>
            <div className="section-head">
              <h2>{t("usage.systemStatus")}</h2>
            </div>
            <div className="stat-row">
              <div className="stat">
                <div className="k">{t("usage.agentsLocal")}</div>
                <div className="v">{localAgents}</div>
              </div>
              <div className="stat">
                <div className="k">{t("usage.agentsCloud")}</div>
                <div className="v">{cloudAgents}</div>
              </div>
              <div className="stat">
                <div className="k">{t("usage.sessionsIndependent")}</div>
                <div className="v">{independentSessions}</div>
              </div>
              <div className="stat">
                <div className="k">{t("usage.sessionsGroup")}</div>
                <div className="v">{groupChats}</div>
              </div>
              <div className="stat">
                <div className="k">{t("usage.providersActive")}</div>
                <div className="v">{distinctProviders}</div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
