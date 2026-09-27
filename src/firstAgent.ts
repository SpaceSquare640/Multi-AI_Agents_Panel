import type { CuratedModel, OllamaModel, RoleTemplate } from "./types";
import { CLOUD_PROVIDERS, isLocalProvider } from "./types";

/** One ready-to-use way to create a first agent, found by looking at what
 *  the user already has: a stored API key, or a running Ollama with at
 *  least one model pulled. */
export interface QuickOption {
  providerName: string;
  model: string;
  /** Shown on the button — the curated label where there is one. */
  modelLabel: string;
}

/** The arguments `create_agent` takes. A type alias rather than an
 *  interface so it can be passed to `invoke` as-is. */
export type CreateAgentArgs = {
  name: string;
  roleTemplate: string | null;
  systemPrompt: string | null;
  providerKind: "local" | "cloud";
  providerName: string;
  model: string;
};

/** Providers whose default is not simply the first curated model.
 *  Anthropic's list leads with the flagship; every built-in role template
 *  suggests Sonnet instead, and a first agent should match what the
 *  templates would have picked rather than the most expensive option. */
const PREFERRED_MODEL: Readonly<Record<string, string>> = { anthropic: "claude-sonnet-5" };

/** The template a Traditional Chinese interface gets. Its prompt requires
 *  every reply to be in Traditional Chinese, which is exactly why no
 *  other interface language gets it. */
export const ZH_HANT_TEMPLATE_ID = "daily-assistant";

/** Every quick-create option the user's current setup supports, cloud
 *  providers first in the app's usual order, then the local model.
 *
 *  A provider with a key but no model to offer is left out rather than
 *  guessed at: a button that creates an agent pointed at a model name the
 *  app made up would fail on the first message, which is worse than not
 *  offering it. */
export function quickCreateOptions(input: {
  providersWithKeys: readonly string[];
  curatedByProvider: Readonly<Record<string, readonly CuratedModel[]>>;
  ollamaModels: readonly OllamaModel[];
}): QuickOption[] {
  const options: QuickOption[] = [];
  for (const provider of CLOUD_PROVIDERS) {
    if (!input.providersWithKeys.includes(provider)) continue;
    const curated = input.curatedByProvider[provider] ?? [];
    const preferred = PREFERRED_MODEL[provider];
    const pick = curated.find((m) => m.id === preferred) ?? curated[0];
    if (pick) {
      options.push({ providerName: provider, model: pick.id, modelLabel: pick.label });
    } else if (preferred) {
      options.push({ providerName: provider, model: preferred, modelLabel: preferred });
    }
  }
  const local = input.ollamaModels[0];
  if (local) {
    options.push({ providerName: "ollama", model: local.name, modelLabel: local.name });
  }
  return options;
}

/** What to create for a chosen option. A Traditional Chinese interface
 *  gets the Daily Assistant template; everything else, or a missing
 *  template, gets a plain agent with no system prompt — one that works
 *  in any language because it asks for none. */
export function quickCreateArgs(
  option: QuickOption,
  language: string | undefined,
  templates: readonly RoleTemplate[],
  genericName: string,
): CreateAgentArgs {
  const template = language === "zh-Hant" ? templates.find((t) => t.id === ZH_HANT_TEMPLATE_ID) : undefined;
  return {
    name: template?.name ?? genericName,
    roleTemplate: template?.name ?? null,
    systemPrompt: template?.systemPrompt ?? null,
    providerKind: isLocalProvider(option.providerName) ? "local" : "cloud",
    providerName: option.providerName,
    model: option.model,
  };
}
