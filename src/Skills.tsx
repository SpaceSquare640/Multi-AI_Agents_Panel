import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import type { Agent, SkillAccessGrant, SkillManifest } from "./types";
import "./Skills.css";

/** Aggregates "which Agents have this Skill granted" from the raw data
 *  three separate `invoke` calls return. Extracted as a pure function
 *  (inputs in, `Map` out — no `invoke`/state) so it's testable without
 *  a fake Tauri backend; `refresh()` below just wires it to the real
 *  calls. Falls back to the agent's id if `byAgentId` somehow doesn't
 *  have its name (shouldn't happen — `agents`/`grantLists` come from
 *  the same fetch — but a missing display name is a much smaller
 *  problem than losing the grant from the list entirely). */
export function aggregateGrantsBySkill(
  agents: Agent[],
  grantLists: SkillAccessGrant[][],
): Map<string, string[]> {
  const byAgentId = new Map(agents.map((a) => [a.id, a.name]));
  const bySkill = new Map<string, string[]>();
  grantLists.forEach((grants, i) => {
    const agentName = byAgentId.get(agents[i].id) ?? agents[i].id;
    for (const grant of grants) {
      const names = bySkill.get(grant.skillName) ?? [];
      names.push(agentName);
      bySkill.set(grant.skillName, names);
    }
  });
  return bySkill;
}

/** Read-only global overview of every discovered Skill and which Agents
 *  currently have it granted. Deliberately does NOT offer a global
 *  enable/disable toggle — Skill access is granted per-Agent (see
 *  `grant_skill_access`/`revoke_skill_access` in Chat.tsx's per-session
 *  Agent Info panel), and there is no backend concept of a Skill being
 *  "on" or "off" App-wide, only which Agents currently hold a grant for
 *  it. A toggle here would imply a control that doesn't actually exist. */
export default function Skills() {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<SkillManifest[]>([]);
  const [grantsBySkill, setGrantsBySkill] = useState<Map<string, string[]>>(new Map());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [exportingName, setExportingName] = useState<string | null>(null);
  const [exportedMessage, setExportedMessage] = useState<string | null>(null);

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

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [manifests, agents] = await Promise.all([
        invoke<SkillManifest[]>("list_skills"),
        invoke<Agent[]>("list_agents"),
      ]);
      setSkills(manifests);

      const grantLists = await Promise.all(
        agents.map((agent) => invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id })),
      );
      setGrantsBySkill(aggregateGrantsBySkill(agents, grantLists));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="skills-screen">
      <div className="skills-head">
        <h1>{t("skills.title")}</h1>
        <div className="skills-head-actions">
          <button onClick={() => void handleImport()} disabled={importing}>
            {importing ? t("skills.importingSkill") : t("skills.importCustomSkill")}
          </button>
          <button onClick={() => void refresh()} disabled={loading}>
            {loading ? t("skills.refreshing") : t("skills.refresh")}
          </button>
        </div>
      </div>
      <p className="acc-hint">{t("skills.hint")}</p>
      <p className="acc-hint">{t("skills.importWarning")}</p>

      {error && <div className="acc-error">{error}</div>}
      {exportedMessage && <div className="acc-hint">{exportedMessage}</div>}

      {!loading && skills.length === 0 && <p className="acc-empty">{t("skills.noneDiscovered")}</p>}

      <div className="skills-grid">
        {skills.map((skill) => {
          const usedBy = grantsBySkill.get(skill.name) ?? [];
          return (
            <div className="skill-card" key={skill.name}>
              <div className="skill-card-top">
                <span className="skill-name">{skill.name}</span>
                <span className={skill.source === "custom" ? "source-tag custom" : "source-tag builtin"}>
                  {skill.source === "custom" ? t("skills.sourceCustom") : t("skills.sourceBuiltin")}
                </span>
              </div>
              <p className="skill-desc">{skill.description}</p>
              <div className="skill-permissions">
                {skill.permissions.length === 0 ? (
                  <span className="permission-tag none">{t("skills.noPermissions")}</span>
                ) : (
                  skill.permissions.map((permission) => (
                    <span className="permission-tag" key={permission}>
                      {permission}
                    </span>
                  ))
                )}
              </div>
              <div className="skill-footer">
                <span className="acc-mono">v{skill.version}</span>
                <span className="agent-count" title={usedBy.length > 0 ? usedBy.join(", ") : undefined}>
                  {usedBy.length === 0 ? t("skills.usedByNone") : t("skills.usedByCount", { count: usedBy.length })}
                </span>
              </div>
              {skill.source === "custom" && (
                <button
                  className="skill-export-button"
                  onClick={() => void handleExport(skill.name)}
                  disabled={exportingName === skill.name}
                >
                  {exportingName === skill.name ? t("skills.exporting") : t("skills.export")}
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
