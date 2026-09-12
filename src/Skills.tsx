import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import { Icon } from "./shell/Icons";
import type { Agent, SkillAccessGrant, SkillManifest } from "./types";
import "./styles/screens/skills.css";

/** For each Skill, the Agents holding a grant for it and the id of the
 *  grant itself: `skillName -> agentId -> grantId`.
 *
 *  Agent ids rather than display names because the chips act on a
 *  specific Agent and two Agents may legitimately share a name; grant
 *  ids because that is what revoking takes — `revoke_skill_access`
 *  identifies the grant row, not the (agent, skill) pair — so a chip
 *  that can turn something off has to carry it.
 *
 *  Pure (grant lists in, map out — no `invoke`, no state) so the
 *  aggregation is testable without a fake Tauri backend; `refresh()`
 *  below just wires it to the real calls. */
export function grantsBySkill(grantLists: SkillAccessGrant[][]): Map<string, Map<string, string>> {
  const bySkill = new Map<string, Map<string, string>>();
  for (const grants of grantLists) {
    for (const grant of grants) {
      const byAgent = bySkill.get(grant.skillName) ?? new Map<string, string>();
      byAgent.set(grant.agentId, grant.id);
      bySkill.set(grant.skillName, byAgent);
    }
  }
  return bySkill;
}

/** Every discovered Skill, what it may do, and which Agents may call it.
 *
 *  Grants are editable here, which is new with the v2 design. There is
 *  still no global on/off — the backend has no such concept, only which
 *  Agents hold a grant — so what the chips expose is the real model:
 *  one grant per Agent per Skill, the same `grant_skill_access` /
 *  `revoke_skill_access` pair the per-session Agent panel in Chat calls.
 *  The previous version of this screen was read-only and sent the user
 *  to Chat to make the change; the design puts the control where the
 *  information is instead. */
export default function Skills() {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<SkillManifest[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [granted, setGranted] = useState<Map<string, Map<string, string>>>(new Map());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [exportingName, setExportingName] = useState<string | null>(null);
  const [exportedMessage, setExportedMessage] = useState<string | null>(null);
  /** `${skillName} ${agentId}` while that one chip's call is in
   *  flight, so a slow backend disables only the chip that was clicked
   *  rather than the whole row. */
  const [pendingGrant, setPendingGrant] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  /** Lets a user add a Skill they got from anywhere on disk (downloaded,
   *  copied from a USB stick, whatever) without needing to find this
   *  app's own install/data directory first — same folder-picker pattern
   *  Chat.tsx's per-Agent panel already used, just surfaced here too
   *  since this is the more natural "manage Skills" screen. */
  async function handleImport() {
    setError(null);
    setExportedMessage(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return; // user cancelled the picker
      setImporting(true);
      await invoke("import_custom_skill", { sourceFolder: folder });
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setImporting(false);
    }
  }

  /** The reverse of import: copies a custom Skill's folder out to a
   *  user-picked destination so it can be shared/backed up, again
   *  without the user needing to locate this app's own data directory. */
  async function handleExport(skillName: string) {
    setError(null);
    setExportedMessage(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return;
      setExportingName(skillName);
      await invoke("export_custom_skill", { skillName, destFolder: folder });
      setExportedMessage(t("skills.exportedTo", { path: `${folder}/${skillName}` }));
    } catch (err) {
      setError(String(err));
    } finally {
      setExportingName(null);
    }
  }

  /** Grant or revoke one Skill for one Agent.
   *
   *  Re-reads the grants from the backend afterwards rather than
   *  flipping the chip locally: the authority on who holds a grant is
   *  the database, and a local flip would keep showing "granted" if the
   *  call had actually failed. */
  async function toggleGrant(skillName: string, agentId: string, grantId: string | undefined) {
    setError(null);
    setPendingGrant(`${skillName} ${agentId}`);
    try {
      if (grantId === undefined) {
        await invoke("grant_skill_access", { agentId, skillName });
      } else {
        // Revoking identifies the grant row, not the (agent, skill) pair,
        // so the chip has to carry the id the aggregation gave it.
        await invoke("revoke_skill_access", { id: grantId });
      }
      await refreshGrants(agents);
    } catch (err) {
      setError(String(err));
    } finally {
      setPendingGrant(null);
    }
  }

  async function refreshGrants(forAgents: Agent[]) {
    const grantLists = await Promise.all(
      forAgents.map((agent) => invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id })),
    );
    setGranted(grantsBySkill(grantLists));
  }

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [manifests, agentList] = await Promise.all([
        invoke<SkillManifest[]>("list_skills"),
        invoke<Agent[]>("list_agents"),
      ]);
      setSkills(manifests);
      setAgents(agentList);
      await refreshGrants(agentList);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <div className="workspace-header">
        <span className="workspace-title">{t("skills.title")}</span>
        <div className="workspace-actions">
          <button className="btn btn-ghost btn-sm" type="button" onClick={() => void refresh()} disabled={loading}>
            {loading ? t("skills.refreshing") : t("skills.refresh")}
          </button>
          <button className="btn btn-secondary btn-sm" type="button" onClick={() => void handleImport()} disabled={importing}>
            <Icon name="plus" size="sm" />
            {importing ? t("skills.importingSkill") : t("skills.importCustomSkill")}
          </button>
        </div>
      </div>

      <div className="workspace-body">
        <div className="pane">
          <p className="pane-intro">{t("skills.hint")}</p>

          <div className="callout" data-kind="info">
            <Icon name="info" />
            <div className="callout-body">
              <strong>{t("skills.grantScopeTitle")}</strong>
              {t("skills.grantScopeBody")}
            </div>
          </div>

          <div className="callout" data-kind="warning">
            <Icon name="alert" />
            <div className="callout-body">{t("skills.importWarning")}</div>
          </div>

          {error && (
            <div className="callout" data-kind="danger">
              <div className="callout-body">{error}</div>
            </div>
          )}

          {exportedMessage && (
            <div className="callout" data-kind="success">
              <div className="callout-body">{exportedMessage}</div>
            </div>
          )}

          {!loading && skills.length === 0 && <p className="pane-intro">{t("skills.noneDiscovered")}</p>}

          {skills.length > 0 && (
            <section>
              <div className="section-head">
                <h2>{t("skills.installed")}</h2>
                <span className="count">{t("skills.skillCount", { count: skills.length })}</span>
              </div>

              {skills.map((skill) => {
                const grantedHere = granted.get(skill.name) ?? new Map<string, string>();
                return (
                  <div className="card skill" key={skill.name}>
                    <div className="skill-head">
                      <div className="skill-title">
                        <h3>{skill.name}</h3>
                        <p>{skill.description}</p>
                      </div>
                      <span className="badge" data-kind={skill.source === "custom" ? "attention" : "succeeded"}>
                        {skill.source === "custom" ? t("skills.sourceCustom") : t("skills.sourceBuiltin")}
                      </span>
                    </div>

                    <div className="scopes">
                      <span className="badge" data-kind="idle">
                        v{skill.version}
                      </span>
                      {skill.permissions.length === 0 ? (
                        <span className="badge" data-kind="idle">
                          {t("skills.noPermissions")}
                        </span>
                      ) : (
                        skill.permissions.map((permission) => (
                          <span className="badge" data-kind="attention" key={permission}>
                            {permission}
                          </span>
                        ))
                      )}
                    </div>

                    {/* Every Agent is a chip, not just the granted ones: the
                        point of the row is that you can tell at a glance who
                        can call this and who cannot, which a list of only the
                        holders cannot show. */}
                    {agents.length === 0 ? (
                      <p className="skill-no-agents">{t("skills.noAgents")}</p>
                    ) : (
                      <div className="grants" role="group" aria-label={t("skills.grantsFor", { skill: skill.name })}>
                        {agents.map((agent) => {
                          const grantId = grantedHere.get(agent.id);
                          const isGranted = grantId !== undefined;
                          const key = `${skill.name} ${agent.id}`;
                          return (
                            <button
                              className="grant"
                              type="button"
                              key={agent.id}
                              aria-pressed={isGranted}
                              disabled={pendingGrant === key}
                              onClick={() => void toggleGrant(skill.name, agent.id, grantId)}
                            >
                              {isGranted && <Icon name="check" size="sm" />}
                              {agent.name}
                            </button>
                          );
                        })}
                      </div>
                    )}

                    {skill.source === "custom" && (
                      <div className="skill-actions">
                        <button
                          className="btn btn-ghost btn-sm"
                          type="button"
                          onClick={() => void handleExport(skill.name)}
                          disabled={exportingName === skill.name}
                        >
                          <Icon name="copy" size="sm" />
                          {exportingName === skill.name ? t("skills.exporting") : t("skills.export")}
                        </button>
                      </div>
                    )}
                  </div>
                );
              })}
            </section>
          )}
        </div>
      </div>
    </>
  );
}
