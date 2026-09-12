import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ask, open as openFilePicker, save as saveFilePicker } from "@tauri-apps/plugin-dialog";
import { Icon } from "./shell/Icons";
import {
  PROVIDER_OPTIONS,
  isLocalProvider,
  type Agent,
  type CuratedModel,
  type OllamaModel,
  type OpenRouterModelsResult,
  type ProviderKeyView,
  type RoleTemplate,
} from "./types";
import "./styles/screens/agents.css";

/** Agents and role templates — creating them, deleting them, and the
 *  "1 人公司" template library they can be created from.
 *
 *  Extracted from Chat when the v2 layout was ported. The design puts
 *  this in the Models sidebar and leaves Chat's sidebar to sessions
 *  alone, which is a real division: a session is something you are doing
 *  now, an agent is something you configured once and reuse. Confirmed
 *  with the user before moving it, because it changes where agents are
 *  created rather than only how the screen looks.
 *
 *  It owns its own data rather than receiving it: Chat still needs the
 *  agent list for its session pickers, but it needs to read it, not to
 *  manage it, and threading twenty pieces of form state up through App
 *  to share them would be worse than two components each calling
 *  `list_agents`. The cost is that an agent created here is not in
 *  Chat's list until Chat refetches, which it does when it next becomes
 *  the visible screen. */
export default function AgentManager({ onError }: { onError: (message: string) => void }) {
  const { t } = useTranslation();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [roleTemplates, setRoleTemplates] = useState<RoleTemplate[]>([]);

  const [showNewAgent, setShowNewAgent] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentTemplateId, setNewAgentTemplateId] = useState("");
  const [newAgentProvider, setNewAgentProvider] = useState<string>("openrouter");
  const [newAgentModel, setNewAgentModel] = useState("");
  const [newAgentModels, setNewAgentModels] = useState<CuratedModel[]>([]);
  const [newAgentSystemPrompt, setNewAgentSystemPrompt] = useState("");
  const [newAgentProviderKeys, setNewAgentProviderKeys] = useState<ProviderKeyView[]>([]);
  const [newAgentPinnedKeyId, setNewAgentPinnedKeyId] = useState("");
  /** True once the user has manually picked a provider in this form
   *  session — after that, selecting a role template stops overwriting
   *  it, since an explicit choice should win over a suggestion. */
  const [newAgentProviderTouched, setNewAgentProviderTouched] = useState(false);

  /** Cross-provider fallback chain, staged locally until the agent is
   *  actually created (`add_agent_fallback_provider` needs a real
   *  agentId) — e.g. Anthropic fails, fall through to OpenRouter. Tried
   *  in this order, only after the primary provider's own key rotation
   *  is exhausted. */
  const [fallbackProvider, setFallbackProvider] = useState<string>("openrouter");
  const [fallbackModel, setFallbackModel] = useState("");
  const [fallbackChain, setFallbackChain] = useState<{ providerKind: string; providerName: string; model: string }[]>(
    [],
  );

  /** The same form creates and edits: `editingTemplateId` set means
   *  editing, null means creating. */
  const [showNewTemplate, setShowNewTemplate] = useState(false);
  const [editingTemplateId, setEditingTemplateId] = useState<string | null>(null);
  const [templateName, setTemplateName] = useState("");
  const [templateDescription, setTemplateDescription] = useState("");
  const [templatePrompt, setTemplatePrompt] = useState("");

  async function refreshAgents() {
    setAgents(await invoke<Agent[]>("list_agents"));
  }

  async function refreshRoleTemplates() {
    const [defaults, custom] = await Promise.all([
      invoke<RoleTemplate[]>("list_default_role_templates"),
      invoke<RoleTemplate[]>("list_custom_role_templates"),
    ]);
    setRoleTemplates([...defaults, ...custom]);
  }

  useEffect(() => {
    refreshAgents().catch((e) => onError(String(e)));
    refreshRoleTemplates().catch((e) => onError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!showNewAgent) return;
    const selectedTemplate = roleTemplates.find((rt) => rt.id === newAgentTemplateId);
    setNewAgentPinnedKeyId("");

    function pickModels(models: CuratedModel[]) {
      setNewAgentModels(models);
      const suggested = selectedTemplate?.suggestedModel;
      const suggestedIsAvailable = suggested && models.some((m) => m.id === suggested);
      setNewAgentModel(suggestedIsAvailable ? suggested : (models[0]?.id ?? ""));
    }

    if (newAgentProvider === "ollama") {
      // Suggesting a model the user hasn't actually pulled is worse than
      // useless — picking it just fails outright, since Ollama has
      // nothing to serve. Show only what's really installed.
      setNewAgentProviderKeys([]);
      invoke<OllamaModel[]>("list_ollama_installed_models")
        .then((models) => pickModels(models.map((m) => ({ id: m.name, label: m.name }))))
        .catch((e) => onError(String(e)));
      return;
    }

    if (isLocalProvider(newAgentProvider)) {
      // colibri/omniroute have no "list what's actually loaded" API to
      // query yet, so the curated list is the best available option.
      setNewAgentProviderKeys([]);
      invoke<CuratedModel[]>("list_curated_models", { provider: newAgentProvider })
        .then(pickModels)
        .catch((e) => onError(String(e)));
      return;
    }

    invoke<ProviderKeyView[]>("list_provider_keys")
      .then((keys) => {
        const providerKeys = keys.filter((k) => k.provider === newAgentProvider);
        setNewAgentProviderKeys(providerKeys);
        // Prefer models the user has actually configured a key/hint for
        // over dumping the whole catalog — a Group Chat with several
        // free-tier OpenRouter keys shouldn't default an Agent to a
        // flagship model none of those keys can afford (the E3001
        // "requires more credits" errors this was causing).
        const hintedModelIds = Array.from(
          new Set(providerKeys.map((k) => k.modelHint).filter((h): h is string => !!h)),
        );
        if (hintedModelIds.length > 0) {
          pickModels(hintedModelIds.map((id) => ({ id, label: id })));
          return;
        }
        if (newAgentProvider === "openrouter") {
          invoke<OpenRouterModelsResult>("list_openrouter_models_live", { forceRefresh: false })
            .then((result) => pickModels(result.models.map((m) => ({ id: m.id, label: m.name }))))
            .catch(() =>
              invoke<CuratedModel[]>("list_curated_models", { provider: newAgentProvider })
                .then(pickModels)
                .catch((e) => onError(String(e))),
            );
          return;
        }
        invoke<CuratedModel[]>("list_curated_models", { provider: newAgentProvider })
          .then(pickModels)
          .catch((e) => onError(String(e)));
      })
      .catch((e) => onError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showNewAgent, newAgentProvider]);

  function handleSelectTemplate(templateId: string) {
    setNewAgentTemplateId(templateId);
    const template = roleTemplates.find((rt) => rt.id === templateId);
    if (!template) {
      setNewAgentSystemPrompt("");
      return;
    }
    if (!newAgentName.trim()) setNewAgentName(template.name);
    setNewAgentSystemPrompt(template.systemPrompt);
    if (template.suggestedProviderName && !newAgentProviderTouched) {
      setNewAgentProvider(template.suggestedProviderName);
    }
  }

  function handleApplyTemplateSuggestion() {
    const template = roleTemplates.find((rt) => rt.id === newAgentTemplateId);
    if (template?.suggestedProviderName) {
      setNewAgentProvider(template.suggestedProviderName);
      setNewAgentProviderTouched(false);
    }
  }

  async function handleCreateAgent(e: FormEvent) {
    e.preventDefault();
    try {
      const selectedTemplate = roleTemplates.find((rt) => rt.id === newAgentTemplateId);
      const agent = await invoke<Agent>("create_agent", {
        name: newAgentName,
        roleTemplate: selectedTemplate?.name ?? null,
        systemPrompt: newAgentSystemPrompt || null,
        providerKind: isLocalProvider(newAgentProvider) ? "local" : "cloud",
        providerName: newAgentProvider,
        model: newAgentModel,
      });
      if (newAgentPinnedKeyId) {
        await invoke("pin_agent_provider_key", { agentId: agent.id, providerKeyId: newAgentPinnedKeyId });
      }
      for (const step of fallbackChain) {
        await invoke("add_agent_fallback_provider", {
          agentId: agent.id,
          providerKind: step.providerKind,
          providerName: step.providerName,
          model: step.model,
        });
      }
      setNewAgentName("");
      setNewAgentSystemPrompt("");
      setNewAgentTemplateId("");
      setNewAgentPinnedKeyId("");
      setNewAgentProviderTouched(false);
      setFallbackChain([]);
      setFallbackModel("");
      setShowNewAgent(false);
      await refreshAgents();
    } catch (err) {
      onError(String(err));
    }
  }

  function handleAddFallbackStep() {
    if (!fallbackModel.trim()) return;
    setFallbackChain((prev) => [
      ...prev,
      {
        providerKind: isLocalProvider(fallbackProvider) ? "local" : "cloud",
        providerName: fallbackProvider,
        model: fallbackModel.trim(),
      },
    ]);
    setFallbackModel("");
  }

  function handleRemoveFallbackStep(index: number) {
    setFallbackChain((prev) => prev.filter((_, i) => i !== index));
  }

  /** Permanently deletes an Agent — there's no undo, so this confirms
   *  first. See `Storage::delete_agent` for what's cascaded (grants,
   *  session membership) vs preserved (messages, usage history, with the
   *  Agent reference nulled out). */
  async function handleDeleteAgent(agentId: string, name: string) {
    const confirmed = await ask(t("chat.deleteAgentConfirm", { name }), {
      title: t("chat.deleteAgentConfirmTitle"),
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await invoke("delete_agent", { agentId });
      await refreshAgents();
    } catch (err) {
      onError(String(err));
    }
  }

  async function handleSaveTemplate(e: FormEvent) {
    e.preventDefault();
    try {
      const args = {
        name: templateName,
        description: templateDescription,
        systemPrompt: templatePrompt,
        suggestedProviderKind: null,
        suggestedProviderName: null,
        suggestedModel: null,
      };
      if (editingTemplateId) {
        await invoke("update_custom_role_template", { id: editingTemplateId, ...args });
      } else {
        await invoke("create_custom_role_template", args);
      }
      handleCancelTemplateForm();
      await refreshRoleTemplates();
    } catch (err) {
      onError(String(err));
    }
  }

  function handleStartEditTemplate(template: RoleTemplate) {
    setEditingTemplateId(template.id);
    setTemplateName(template.name);
    setTemplateDescription(template.description);
    setTemplatePrompt(template.systemPrompt);
    setShowNewTemplate(true);
  }

  function handleCancelTemplateForm() {
    setEditingTemplateId(null);
    setTemplateName("");
    setTemplateDescription("");
    setTemplatePrompt("");
    setShowNewTemplate(false);
  }

  async function handleDeleteTemplate(id: string) {
    try {
      await invoke("delete_custom_role_template", { id });
      await refreshRoleTemplates();
    } catch (err) {
      onError(String(err));
    }
  }

  async function handleExportTemplate(template: RoleTemplate) {
    try {
      const destPath = await saveFilePicker({
        defaultPath: `${template.name.replace(/[^a-zA-Z0-9 _-]/g, "_")}.json`,
        filters: [{ name: "Role Template", extensions: ["json"] }],
      });
      if (!destPath) return; // user cancelled the picker
      await invoke("export_custom_role_template", { id: template.id, destPath });
    } catch (err) {
      onError(String(err));
    }
  }

  async function handleImportTemplate() {
    try {
      const sourcePath = await openFilePicker({
        directory: false,
        multiple: false,
        filters: [{ name: "Role Template", extensions: ["json"] }],
      });
      if (!sourcePath) return; // user cancelled the picker
      await invoke("import_custom_role_template", { sourcePath });
      await refreshRoleTemplates();
    } catch (err) {
      onError(String(err));
    }
  }

  const customTemplates = roleTemplates.filter((rt) => rt.source === "custom");
  const suggestedProvider = roleTemplates.find((rt) => rt.id === newAgentTemplateId)?.suggestedProviderName;

  return (
    <>
      <div className="sidebar-header">
        <span className="label">{t("chat.agents")}</span>
        <button
          className="titlebar-btn"
          type="button"
          aria-label={t("chat.newAgent")}
          aria-pressed={showNewAgent}
          onClick={() => {
            setShowNewAgent((v) => !v);
            setNewAgentProviderTouched(false);
          }}
        >
          <Icon name="plus" size="sm" />
        </button>
      </div>

      <div className="sidebar-body">
        {agents.map((a) => (
          <div className="agent-row" key={a.id}>
            <span className="row row-tall agent-row-main">
              <Icon name={a.providerKind === "local" ? "local" : "cloud"} size="sm" />
              <span className="name">
                {a.name}
                <span className="row-sub">
                  {a.providerName}/{a.model}
                </span>
              </span>
            </span>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.deleteAgent", { name: a.name })}
              onClick={() => void handleDeleteAgent(a.id, a.name)}
            >
              <Icon name="trash" size="sm" />
            </button>
          </div>
        ))}
        {agents.length === 0 && <p className="tree-empty">{t("chat.noAgentsYet")}</p>}

        {showNewAgent && (
          <form className="agent-form" onSubmit={handleCreateAgent}>
            <select className="select" value={newAgentTemplateId} onChange={(e) => handleSelectTemplate(e.target.value)}>
              <option value="">{t("chat.noRoleTemplate")}</option>
              {roleTemplates.map((rt) => (
                <option key={rt.id} value={rt.id}>
                  {rt.name} {rt.source === "custom" ? t("chat.custom") : ""}
                </option>
              ))}
            </select>
            <input
              className="input"
              type="text"
              placeholder={t("chat.agentNamePlaceholder")}
              value={newAgentName}
              onChange={(e) => setNewAgentName(e.target.value)}
              required
            />
            <select
              className="select"
              value={newAgentProvider}
              onChange={(e) => {
                setNewAgentProvider(e.target.value);
                setNewAgentProviderTouched(true);
              }}
            >
              {PROVIDER_OPTIONS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            {suggestedProvider && suggestedProvider !== newAgentProvider && (
              <button className="btn btn-ghost btn-sm" type="button" onClick={handleApplyTemplateSuggestion}>
                {t("chat.applySuggestedProvider", { provider: suggestedProvider })}
              </button>
            )}
            {newAgentProvider === "ollama" && newAgentModels.length === 0 ? (
              <p className="field-hint">{t("chat.noOllamaModelsInstalled")}</p>
            ) : (
              <select className="select" value={newAgentModel} onChange={(e) => setNewAgentModel(e.target.value)}>
                {newAgentModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
            )}
            <textarea
              className="textarea"
              rows={3}
              placeholder={t("chat.systemPromptPlaceholder")}
              value={newAgentSystemPrompt}
              onChange={(e) => setNewAgentSystemPrompt(e.target.value)}
            />
            {!isLocalProvider(newAgentProvider) && (
              <select
                className="select"
                value={newAgentPinnedKeyId}
                onChange={(e) => setNewAgentPinnedKeyId(e.target.value)}
              >
                <option value="">{t("chat.useLatestKeyDefault", { provider: newAgentProvider })}</option>
                {newAgentProviderKeys.map((k) => (
                  <option key={k.id} value={k.id}>
                    {t("chat.pinTo", { label: k.label ?? k.maskedSecret })}
                  </option>
                ))}
              </select>
            )}

            <div className="fallback-editor">
              <span className="label">{t("chat.fallbackLabel")}</span>
              {fallbackChain.length === 0 && <span className="field-hint">{t("chat.noneConfigured")}</span>}
              {fallbackChain.map((step, i) => (
                <span className="grant" key={i} aria-pressed="true">
                  {i + 1}. {step.providerName}/{step.model}
                  <button
                    className="chip-remove"
                    type="button"
                    onClick={() => handleRemoveFallbackStep(i)}
                    aria-label={t("chat.removeFallbackStep", { step: `${step.providerName}/${step.model}` })}
                  >
                    ×
                  </button>
                </span>
              ))}
              <div className="fallback-add">
                <select className="select" value={fallbackProvider} onChange={(e) => setFallbackProvider(e.target.value)}>
                  {PROVIDER_OPTIONS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                <input
                  className="input"
                  type="text"
                  placeholder={t("chat.modelIdPlaceholder")}
                  value={fallbackModel}
                  onChange={(e) => setFallbackModel(e.target.value)}
                />
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  disabled={!fallbackModel.trim()}
                  onClick={() => handleAddFallbackStep()}
                >
                  {t("chat.addFallback")}
                </button>
              </div>
            </div>

            <button className="btn btn-primary btn-sm" type="submit">
              {t("chat.createAgent")}
            </button>
          </form>
        )}

        <div className="label sidebar-section-label">{t("chat.customRoleTemplates")}</div>
        {customTemplates.map((rt) => (
          <div className="agent-row" key={rt.id}>
            <span className="row agent-row-main" title={rt.description}>
              <span className="name">{rt.name}</span>
            </span>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.edit")}
              onClick={() => handleStartEditTemplate(rt)}
            >
              <Icon name="settings" size="sm" />
            </button>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.export")}
              onClick={() => void handleExportTemplate(rt)}
            >
              <Icon name="copy" size="sm" />
            </button>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.delete")}
              onClick={() => void handleDeleteTemplate(rt.id)}
            >
              <Icon name="trash" size="sm" />
            </button>
          </div>
        ))}
        {customTemplates.length === 0 && <p className="tree-empty">{t("chat.noCustomTemplates")}</p>}

        <div className="sidebar-actions">
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            onClick={() => (showNewTemplate ? handleCancelTemplateForm() : setShowNewTemplate(true))}
          >
            {showNewTemplate ? t("chat.cancel") : t("chat.newRoleTemplate")}
          </button>
          <button className="btn btn-ghost btn-sm" type="button" onClick={() => void handleImportTemplate()}>
            {t("chat.importTemplate")}
          </button>
        </div>

        {showNewTemplate && (
          <form className="agent-form" onSubmit={handleSaveTemplate}>
            <input
              className="input"
              type="text"
              placeholder={t("chat.templateNamePlaceholder")}
              value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
              required
            />
            <input
              className="input"
              type="text"
              placeholder={t("chat.shortDescriptionPlaceholder")}
              value={templateDescription}
              onChange={(e) => setTemplateDescription(e.target.value)}
              required
            />
            <textarea
              className="textarea"
              rows={3}
              placeholder={t("chat.systemPromptPlainPlaceholder")}
              value={templatePrompt}
              onChange={(e) => setTemplatePrompt(e.target.value)}
              required
            />
            <button className="btn btn-primary btn-sm" type="submit">
              {editingTemplateId ? t("chat.saveChanges") : t("chat.saveTemplate")}
            </button>
          </form>
        )}
      </div>

      <div className="sidebar-footer">
        <div className="row">
          <span className="name">{t("chat.agentCount", { count: agents.length })}</span>
        </div>
      </div>
    </>
  );
}
