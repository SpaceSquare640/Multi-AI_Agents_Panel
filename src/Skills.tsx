import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent, SkillAccessGrant, SkillManifest } from "./types";
import "./Skills.css";

/** Read-only global overview of every discovered Skill and which Agents
 *  currently have it granted. Deliberately does NOT offer a global
 *  enable/disable toggle — Skill access is granted per-Agent (see
 *  `grant_skill_access`/`revoke_skill_access` in Chat.tsx's per-session
 *  Agent Info panel), and there is no backend concept of a Skill being
 *  "on" or "off" App-wide, only which Agents currently hold a grant for
 *  it. A toggle here would imply a control that doesn't actually exist. */
export default function Skills() {
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
      setGrantsBySkill(bySkill);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="skills-screen">
      <div className="skills-head">
        <h1>Skills</h1>
        <button onClick={() => void refresh()} disabled={loading}>
          {loading ? "Refreshing…" : "Refresh"}
        </button>
      </div>
      <p className="acc-hint">
        Skill access is granted per-Agent — open an Agent's session in Chat to grant or revoke a
        Skill for it. This page just shows what's currently in use across every Agent.
      </p>

      {error && <div className="acc-error">{error}</div>}

      {!loading && skills.length === 0 && <p className="acc-empty">No Skills discovered.</p>}

      <div className="skills-grid">
        {skills.map((skill) => {
          const usedBy = grantsBySkill.get(skill.name) ?? [];
          return (
            <div className="skill-card" key={skill.name}>
              <div className="skill-card-top">
                <span className="skill-name">{skill.name}</span>
                <span className={skill.source === "custom" ? "source-tag custom" : "source-tag builtin"}>
                  {skill.source === "custom" ? "Custom" : "Built-in"}
                </span>
              </div>
              <p className="skill-desc">{skill.description}</p>
              <div className="skill-footer">
                <span className="acc-mono">v{skill.version}</span>
                <span className="agent-count" title={usedBy.length > 0 ? usedBy.join(", ") : undefined}>
                  {usedBy.length === 0 ? "Not used by any Agent" : `Used by ${usedBy.length} Agent${usedBy.length === 1 ? "" : "s"}`}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
