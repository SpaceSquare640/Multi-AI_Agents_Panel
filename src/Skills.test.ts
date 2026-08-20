import { describe, expect, it } from "vitest";
import { aggregateGrantsBySkill } from "./Skills";
import type { Agent, SkillAccessGrant } from "./types";

function agent(id: string, name: string): Agent {
  return {
    id,
    name,
    roleTemplate: null,
    systemPrompt: null,
    providerKind: "local",
    providerName: "ollama",
    model: "llama3.1:8b",
    pinnedProviderKeyId: null,
    createdAt: "2026-01-01T00:00:00Z",
  };
}

function grant(agentId: string, skillName: string): SkillAccessGrant {
  return { id: `${agentId}-${skillName}`, agentId, skillName, grantedAt: "2026-01-01T00:00:00Z" };
}

describe("aggregateGrantsBySkill", () => {
  it("maps each Skill to the names of every Agent that has it granted", () => {
    const agents = [agent("a1", "Research Assistant"), agent("a2", "Copywriter")];
    const grantLists = [
      [grant("a1", "web_search"), grant("a1", "semantic_search")],
      [grant("a2", "web_search")],
    ];

    const result = aggregateGrantsBySkill(agents, grantLists);

    expect(result.get("web_search")).toEqual(["Research Assistant", "Copywriter"]);
    expect(result.get("semantic_search")).toEqual(["Research Assistant"]);
  });

  it("returns an empty map when no Agent has any grants", () => {
    const agents = [agent("a1", "Research Assistant")];
    expect(aggregateGrantsBySkill(agents, [[]]).size).toBe(0);
  });

  it("handles no Agents at all", () => {
    expect(aggregateGrantsBySkill([], []).size).toBe(0);
  });
});
