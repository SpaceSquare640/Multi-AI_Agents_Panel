import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
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

  useEffect(() => {
    void refresh();
  }, []);

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
        <button onClick={() => void refresh()} disabled={loading}>
          {loading ? t("skills.refreshing") : t("skills.refresh")}
        </button>
      </div>
      <p className="acc-hint">{t("skills.hint")}</p>

      {error && <div className="acc-error">{error}</div>}

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
            </div>
          );
        })}
      </div>
    </div>
  );
}
