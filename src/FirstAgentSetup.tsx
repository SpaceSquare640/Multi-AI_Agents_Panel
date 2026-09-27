import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { Agent, CuratedModel, OllamaModel, ProviderKeyView, RoleTemplate } from "./types";
import { CLOUD_PROVIDERS } from "./types";
import { quickCreateArgs, quickCreateOptions, type QuickOption } from "./firstAgent";

/** Brand names, so not translated. */
const PROVIDER_LABELS: Readonly<Record<string, string>> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  ollama: "Ollama",
};

/** What someone with no agents sees instead of a dead end.
 *
 *  Used in two places — Onboarding's second step and Chat's empty state —
 *  so that there is one way to make a first agent rather than two forms
 *  that drift apart. The full form stays where it always was, in the
 *  Models screen; this only offers the options the user's existing setup
 *  already makes work, and a way to that form when there are none. */
export default function FirstAgentSetup({
  onCreated,
  onOpenModels,
  headingLevel = 2,
  headingId,
}: {
  onCreated: (agent: Agent) => void;
  onOpenModels: () => void;
  /** 1 inside Onboarding, where this is the dialog's title. */
  headingLevel?: 1 | 2;
  headingId?: string;
}) {
  const { t, i18n } = useTranslation();
  const [options, setOptions] = useState<QuickOption[] | null>(null);
  const [templates, setTemplates] = useState<RoleTemplate[]>([]);
  const [creating, setCreating] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function detect() {
      // Each probe fails on its own terms. An unreadable key list must not
      // hide a running Ollama, and an Ollama that is not installed is the
      // ordinary case, not an error — both simply mean "no option here".
      const [keys, defaults, ollamaRunning] = await Promise.all([
        invoke<ProviderKeyView[]>("list_provider_keys").catch(() => [] as ProviderKeyView[]),
        invoke<RoleTemplate[]>("list_default_role_templates").catch(() => [] as RoleTemplate[]),
        invoke<boolean>("ollama_is_running").catch(() => false),
      ]);
      const providersWithKeys = [...new Set(keys.map((k) => k.provider))].filter((p) =>
        (CLOUD_PROVIDERS as readonly string[]).includes(p),
      );
      const curated = await Promise.all(
        providersWithKeys.map(
          async (provider) =>
            [
              provider,
              await invoke<CuratedModel[]>("list_curated_models", { provider }).catch(() => [] as CuratedModel[]),
            ] as const,
        ),
      );
      const ollamaModels = ollamaRunning
        ? await invoke<OllamaModel[]>("list_ollama_installed_models").catch(() => [] as OllamaModel[])
        : [];
      if (cancelled) return;
      setTemplates(defaults);
      setOptions(
        quickCreateOptions({ providersWithKeys, curatedByProvider: Object.fromEntries(curated), ollamaModels }),
      );
    }
    void detect();
    return () => {
      cancelled = true;
    };
  }, []);

  async function create(option: QuickOption) {
    setError(null);
    setCreating(option.providerName);
    try {
      const args = quickCreateArgs(option, i18n.resolvedLanguage, templates, t("firstAgent.defaultName"));
      onCreated(await invoke<Agent>("create_agent", args));
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(null);
    }
  }

  const screen = t("shell.nav.models");
  const Heading = headingLevel === 1 ? "h1" : "h2";

  return (
    <div className="first-agent">
      <Heading id={headingId} className="first-agent-title">
        {t("firstAgent.title")}
      </Heading>
      <p className="pane-intro">{t("firstAgent.lead")}</p>

      {options === null && <p className="field-hint">{t("firstAgent.checking")}</p>}
      {options !== null && options.length === 0 && (
        <p className="field-hint">{t("firstAgent.noneFound", { screen })}</p>
      )}
      {options !== null && options.length > 0 && (
        <div className="first-agent-options">
          {options.map((option) => (
            <button
              key={option.providerName}
              className="btn btn-primary"
              type="button"
              disabled={creating !== null}
              onClick={() => void create(option)}
            >
              {creating === option.providerName
                ? t("firstAgent.creating")
                : t("firstAgent.createWith", {
                    provider: PROVIDER_LABELS[option.providerName] ?? option.providerName,
                    model: option.modelLabel,
                  })}
            </button>
          ))}
        </div>
      )}

      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}

      <button className="btn btn-ghost" type="button" onClick={onOpenModels}>
        {t("firstAgent.openModels", { screen })}
      </button>
    </div>
  );
}
