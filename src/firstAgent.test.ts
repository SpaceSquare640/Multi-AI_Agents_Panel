import { describe, expect, it } from "vitest";
import { quickCreateArgs, quickCreateOptions, type QuickOption } from "./firstAgent";
import type { RoleTemplate } from "./types";

const ANTHROPIC = [
  { id: "claude-fable-5-1", label: "Claude Fable 5.1" },
  { id: "claude-opus-5", label: "Claude Opus 5" },
  { id: "claude-sonnet-5", label: "Claude Sonnet 5" },
  { id: "claude-haiku-4-5", label: "Claude Haiku 4.5" },
];
const OPENAI = [
  { id: "gpt-a", label: "GPT A" },
  { id: "gpt-b", label: "GPT B" },
];

describe("quickCreateOptions", () => {
  it("offers nothing when there is no key and no local model", () => {
    expect(quickCreateOptions({ providersWithKeys: [], curatedByProvider: {}, ollamaModels: [] })).toEqual([]);
  });

  it("picks Sonnet for Anthropic, matching the built-in templates, not the first model in the list", () => {
    const [option] = quickCreateOptions({
      providersWithKeys: ["anthropic"],
      curatedByProvider: { anthropic: ANTHROPIC },
      ollamaModels: [],
    });
    expect(option).toEqual({ providerName: "anthropic", model: "claude-sonnet-5", modelLabel: "Claude Sonnet 5" });
  });

  it("falls back to the first curated model when the preferred one is no longer listed", () => {
    const [option] = quickCreateOptions({
      providersWithKeys: ["anthropic"],
      curatedByProvider: { anthropic: [{ id: "claude-next", label: "Claude Next" }] },
      ollamaModels: [],
    });
    expect(option.model).toBe("claude-next");
  });

  it("uses the first curated model for providers without a preference", () => {
    const [option] = quickCreateOptions({
      providersWithKeys: ["openai"],
      curatedByProvider: { openai: OPENAI },
      ollamaModels: [],
    });
    expect(option).toEqual({ providerName: "openai", model: "gpt-a", modelLabel: "GPT A" });
  });

  it("leaves out a provider that has a key but no model to offer, rather than guessing one", () => {
    expect(
      quickCreateOptions({ providersWithKeys: ["openai"], curatedByProvider: { openai: [] }, ollamaModels: [] }),
    ).toEqual([]);
  });

  it("ignores keys for providers the app does not chat with", () => {
    expect(
      quickCreateOptions({ providersWithKeys: ["someone-else"], curatedByProvider: {}, ollamaModels: [] }),
    ).toEqual([]);
  });

  it("offers the first installed Ollama model, after the cloud options", () => {
    const options = quickCreateOptions({
      providersWithKeys: ["openai"],
      curatedByProvider: { openai: OPENAI },
      ollamaModels: [
        { name: "llama3.1:8b", size: null, modifiedAt: null },
        { name: "qwen3:4b", size: null, modifiedAt: null },
      ],
    });
    expect(options.map((o) => `${o.providerName}/${o.model}`)).toEqual(["openai/gpt-a", "ollama/llama3.1:8b"]);
  });
});

describe("quickCreateArgs", () => {
  const daily: RoleTemplate = {
    id: "daily-assistant",
    name: "Daily Assistant",
    description: "",
    systemPrompt: "一律使用繁體中文回覆",
    suggestedProviderKind: "cloud",
    suggestedProviderName: "anthropic",
    suggestedModel: "claude-sonnet-5",
    source: "default",
  };
  const anthropic: QuickOption = { providerName: "anthropic", model: "claude-sonnet-5", modelLabel: "Claude Sonnet 5" };
  const ollama: QuickOption = { providerName: "ollama", model: "llama3.1:8b", modelLabel: "llama3.1:8b" };

  it("gives a Traditional Chinese interface the Daily Assistant template", () => {
    expect(quickCreateArgs(anthropic, "zh-Hant", [daily], "助理")).toEqual({
      name: "Daily Assistant",
      roleTemplate: "Daily Assistant",
      systemPrompt: "一律使用繁體中文回覆",
      providerKind: "cloud",
      providerName: "anthropic",
      model: "claude-sonnet-5",
    });
  });

  it("gives every other language a plain agent, since the template demands Traditional Chinese replies", () => {
    for (const language of ["en", "zh-Hans", "ja", "ko", "fr", "de", undefined]) {
      const args = quickCreateArgs(anthropic, language, [daily], "Assistant");
      expect(args.systemPrompt).toBeNull();
      expect(args.roleTemplate).toBeNull();
      expect(args.name).toBe("Assistant");
    }
  });

  it("falls back to a plain agent when the template is missing", () => {
    expect(quickCreateArgs(anthropic, "zh-Hant", [], "助理").name).toBe("助理");
  });

  it("marks Ollama agents as local", () => {
    expect(quickCreateArgs(ollama, "en", [], "Assistant").providerKind).toBe("local");
  });
});
