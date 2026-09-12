import { describe, expect, it } from "vitest";
import { grantsBySkill } from "./Skills";
import type { SkillAccessGrant } from "./types";

function grant(agentId: string, skillName: string): SkillAccessGrant {
  return { id: `${agentId}-${skillName}`, agentId, skillName, grantedAt: "2026-01-01T00:00:00Z" };
}

describe("grantsBySkill", () => {
  it("maps each Skill to the Agents holding it, and to the id of each grant", () => {
    const grantLists = [
      [grant("a1", "web_search"), grant("a1", "semantic_search")],
      [grant("a2", "web_search")],
    ];

    const result = grantsBySkill(grantLists);

    expect([...result.get("web_search")!.entries()]).toEqual([
      ["a1", "a1-web_search"],
      ["a2", "a2-web_search"],
    ]);
    expect([...result.get("semantic_search")!.keys()]).toEqual(["a1"]);
  });

  /* The id is what revoking takes, so carrying the wrong one would
     revoke a different Agent's grant rather than fail visibly. */
  it("keeps each Agent's own grant id rather than the last one seen", () => {
    const result = grantsBySkill([[grant("a1", "shared")], [grant("a2", "shared")]]);
    expect(result.get("shared")!.get("a1")).toBe("a1-shared");
    expect(result.get("shared")!.get("a2")).toBe("a2-shared");
  });

  it("returns an empty map when no Agent has any grants", () => {
    expect(grantsBySkill([[]]).size).toBe(0);
  });

  it("handles no Agents at all", () => {
    expect(grantsBySkill([]).size).toBe(0);
  });
});
