import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker, save as saveFilePicker } from "@tauri-apps/plugin-dialog";
import type {
  Agent,
  CuratedModel,
  FileAccessGrant,
  GroupTurnResult,
  Message,
  MlAccessGrant,
  MlCapabilityManifest,
  ProviderKeyView,
  RoleTemplate,
  SemanticSearchResult,
  Session,
  SkillAccessGrant,
  SkillManifest,
} from "./types";
import "./Chat.css";

const PROVIDER_OPTIONS = ["anthropic", "openai", "openrouter", "ollama", "colibri"] as const;

/// Providers that run as a local server the user starts themselves —
/// no Key Vault entry to pick/pin, unlike the cloud providers above.
const LOCAL_PROVIDERS = ["ollama", "colibri"] as const;
function isLocalProvider(provider: string): boolean {
  return (LOCAL_PROVIDERS as readonly string[]).includes(provider);
}

/// Per-session state, kept independently for every *open* tab so that
/// sending a message in one session never blocks, resets, or loses state
/// in another — this is what makes "multiple agents in parallel" real
/// rather than just a session picker. See dev order step 10 /
/// Session Types.md.
interface TabState {
  /** "independent" | "group" — decides which send/turn commands this tab uses. */
  kind: string;
  messages: Message[];
  /** Independent Session's single agent. Null for group tabs — see `members`. */
  agent: Agent | null;
  /** Group Chat's participants, in round-robin (join) order. Empty for independent tabs. */
  members: Agent[];
  fileGrants: FileAccessGrant[];
  /** Skills this tab's agent may call. Empty for group tabs (not wired up yet — see Backlog). */
  skillGrants: SkillAccessGrant[];
  /** ML capabilities (e.g. semantic_search) this tab's agent may call.
   *  Independent Sessions only for now — Group Chat semantic search needs
   *  File Access sharing to land first (see Backlog). */
  mlGrants: MlAccessGrant[];
  searchResults: SemanticSearchResult[] | null;
  draft: string;
  sending: boolean;
  /** True while this tab is open but not the one currently in view — used
   *  to show a "new reply" dot without disturbing the active tab. */
  hasUnseenReply: boolean;
  /** Set when a Group Chat turn paused for the local→cloud boundary
   *  confirmation (E6004, see Orchestration Design.md) — the content
   *  that would be sent to a cloud Agent. Null when nothing is pending. */
  pendingBoundary: string | null;
  /** Which action to retry after the user confirms `pendingBoundary` —
   *  a regular turn (`advance_group_turn`) or ending the meeting
   *  (`end_group_chat_meeting`), since both can trigger E6004. */
  pendingBoundaryRetry: "turn" | "endMeeting" | null;
}

function emptyTab(): TabState {
  return {
    kind: "independent",
    messages: [],
    agent: null,
    members: [],
    fileGrants: [],
    skillGrants: [],
    mlGrants: [],
    searchResults: null,
    draft: "",
    sending: false,
    hasUnseenReply: false,
    pendingBoundary: null,
    pendingBoundaryRetry: null,
  };
}

/// Every error surfaced from the Rust side is formatted as
/// `"${error_code} ${message}"` (see `ProviderError`'s `Display` impl
/// and the Error Code Registry) — this pulls that code back out so the
/// UI can show it as its own chip instead of buried in prose, per
/// Design Principles' "錯誤訊息一律帶錯誤代號" rule. Errors that don't
/// match the pattern (e.g. a plain client-side validation message like
/// "Create an agent first.") just render with no code chip.
export function parseErrorCode(message: string): { code: string | null; rest: string } {
  const match = /^(E\d{4})\s+(.*)$/s.exec(message);
  return match ? { code: match[1], rest: match[2] } : { code: null, rest: message };
}

export default function Chat() {
  const { t } = useTranslation();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [openTabIds, setOpenTabIds] = useState<string[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [tabs, setTabs] = useState<Record<string, TabState>>({});
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Skills: the installed catalog (global) + per-tab grants (in TabState)
  // + a small "run one now" form scoped to whichever tab is active.
  const [availableSkills, setAvailableSkills] = useState<SkillManifest[]>([]);
  const [skillToGrant, setSkillToGrant] = useState("");
  const [runSkillName, setRunSkillName] = useState("");
  const [runSkillPayload, setRunSkillPayload] = useState("{}");
  const [runningSkill, setRunningSkill] = useState(false);
  const [importingSkill, setImportingSkill] = useState(false);

  // Semantic search (ml_engine): same catalog + per-tab-grants pattern as
  // Skills. Index name is always the agent's id for now — Group Chat's
  // shared index naming (`group-<sessionId>`) waits on File Access
  // sharing landing first, see Backlog.
  const [availableMlCapabilities, setAvailableMlCapabilities] = useState<MlCapabilityManifest[]>([]);
  const [mlCapabilityToGrant, setMlCapabilityToGrant] = useState("");
  const [indexing, setIndexing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);

  // Group Chat "let them keep talking for N turns" — see handleAutoContinue.
  const [autoContinueTurns, setAutoContinueTurns] = useState(3);

  const activeTab = activeSessionId ? tabs[activeSessionId] : undefined;

  function patchTab(sessionId: string, patch: Partial<TabState>) {
    setTabs((prev) => ({ ...prev, [sessionId]: { ...(prev[sessionId] ?? emptyTab()), ...patch } }));
  }

  // New-agent form state.
  const [showNewAgent, setShowNewAgent] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentProvider, setNewAgentProvider] = useState<string>("openrouter");
  const [newAgentModels, setNewAgentModels] = useState<CuratedModel[]>([]);
  const [newAgentModel, setNewAgentModel] = useState("");
  const [newAgentSystemPrompt, setNewAgentSystemPrompt] = useState("");
  const [newAgentTemplateId, setNewAgentTemplateId] = useState("");
  const [newAgentProviderKeys, setNewAgentProviderKeys] = useState<ProviderKeyView[]>([]);
  const [newAgentPinnedKeyId, setNewAgentPinnedKeyId] = useState("");
  // True once the user has manually picked a provider in this form session —
  // once set, selecting a role template stops overwriting it, since the
  // user's explicit choice should win over the template's suggestion.
  const [newAgentProviderTouched, setNewAgentProviderTouched] = useState(false);
  // Cross-provider fallback chain, staged locally until the agent is
  // actually created (add_agent_fallback_provider needs a real agentId) —
  // e.g. Anthropic fails, fall through to OpenRouter. Tried in this order,
  // only after the primary provider's own key rotation is exhausted.
  const [fallbackProvider, setFallbackProvider] = useState<string>("openrouter");
  const [fallbackModel, setFallbackModel] = useState("");
  const [fallbackChain, setFallbackChain] = useState<{ providerKind: string; providerName: string; model: string }[]>(
    [],
  );

  // Role templates ("1 人公司"): default (built-in) + custom (user-authored).
  // The same form handles both creating a new template and editing an
  // existing one — `editingTemplateId` set means "editing", null means
  // "creating a new one".
  const [roleTemplates, setRoleTemplates] = useState<RoleTemplate[]>([]);
  const [showNewTemplate, setShowNewTemplate] = useState(false);
  const [editingTemplateId, setEditingTemplateId] = useState<string | null>(null);
  const [templateName, setTemplateName] = useState("");
  const [templateDescription, setTemplateDescription] = useState("");
  const [templatePrompt, setTemplatePrompt] = useState("");

  // New-session form state.
  const [newSessionAgentId, setNewSessionAgentId] = useState("");
  const [newSessionTitle, setNewSessionTitle] = useState("");

  // New-group-session form state.
  const [showNewGroup, setShowNewGroup] = useState(false);
  const [newGroupTitle, setNewGroupTitle] = useState("");
  const [newGroupAgentIds, setNewGroupAgentIds] = useState<string[]>([]);

  async function refreshAgents() {
    const list = await invoke<Agent[]>("list_agents");
    setAgents(list);
    if (list.length > 0 && !newSessionAgentId) setNewSessionAgentId(list[0].id);
  }

  async function refreshSessions() {
    const all = await invoke<Session[]>("list_sessions");
    setSessions(all);
  }

  const independentSessions = sessions.filter((s) => s.kind === "independent");
  const groupSessions = sessions.filter((s) => s.kind === "group");

  async function refreshRoleTemplates() {
    const [defaults, custom] = await Promise.all([
      invoke<RoleTemplate[]>("list_default_role_templates"),
      invoke<RoleTemplate[]>("list_custom_role_templates"),
    ]);
    setRoleTemplates([...defaults, ...custom]);
  }

  useEffect(() => {
    refreshAgents().catch((e) => setError(String(e)));
    refreshSessions().catch((e) => setError(String(e)));
    refreshRoleTemplates().catch((e) => setError(String(e)));
    invoke<SkillManifest[]>("list_skills")
      .then(setAvailableSkills)
      .catch((e) => setError(String(e)));
    invoke<MlCapabilityManifest[]>("list_ml_capabilities")
      .then(setAvailableMlCapabilities)
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [activeTab?.messages]);

  useEffect(() => {
    if (!showNewAgent) return;
    const selectedTemplate = roleTemplates.find((t) => t.id === newAgentTemplateId);
    invoke<CuratedModel[]>("list_curated_models", { provider: newAgentProvider })
      .then((models) => {
        setNewAgentModels(models);
        const suggested = selectedTemplate?.suggestedModel;
        const suggestedIsAvailable = suggested && models.some((m) => m.id === suggested);
        setNewAgentModel(suggestedIsAvailable ? suggested : models[0]?.id ?? "");
      })
      .catch((e) => setError(String(e)));
    setNewAgentPinnedKeyId("");
    if (isLocalProvider(newAgentProvider)) {
      setNewAgentProviderKeys([]);
    } else {
      invoke<ProviderKeyView[]>("list_provider_keys")
        .then((keys) => setNewAgentProviderKeys(keys.filter((k) => k.provider === newAgentProvider)))
        .catch((e) => setError(String(e)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showNewAgent, newAgentProvider]);

  function handleSelectTemplate(templateId: string) {
    setNewAgentTemplateId(templateId);
    const template = roleTemplates.find((t) => t.id === templateId);
    if (!template) {
      setNewAgentSystemPrompt("");
      return;
    }
    if (!newAgentName.trim()) setNewAgentName(template.name);
    setNewAgentSystemPrompt(template.systemPrompt);
    // Only auto-apply the suggested provider if the user hasn't manually
    // picked one yet in this form session — a manual choice should never be
    // silently overwritten by picking a template afterwards.
    if (template.suggestedProviderName && !newAgentProviderTouched) {
      setNewAgentProvider(template.suggestedProviderName);
    }
  }

  function handleApplyTemplateSuggestion() {
    const template = roleTemplates.find((t) => t.id === newAgentTemplateId);
    if (template?.suggestedProviderName) {
      setNewAgentProvider(template.suggestedProviderName);
      setNewAgentProviderTouched(false);
    }
  }

  async function handleCreateAgent(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const selectedTemplate = roleTemplates.find((t) => t.id === newAgentTemplateId);
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
      // Fallback chain steps are staged locally (see fallbackChain state)
      // since add_agent_fallback_provider needs a real agentId — write
      // them in order now that the agent actually exists.
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
      setNewSessionAgentId(agent.id);
    } catch (err) {
      setError(String(err));
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

  /// Opens a session as a tab (loading its messages/agent(s)/grants the
  /// first time) and brings it to the front. Already-open tabs keep
  /// whatever state they had — switching tabs never re-fetches or resets.
  async function openTab(sessionId: string, knownKind?: string) {
    setActiveSessionId(sessionId);
    patchTab(sessionId, { hasUnseenReply: false });
    setOpenTabIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));
    if (tabs[sessionId]) return; // already loaded

    // `sessions` may not have re-rendered yet if this is called right
    // after creating the session (state updates aren't synchronous), so
    // a freshly created session's kind is passed in explicitly.
    const kind = knownKind ?? sessions.find((s) => s.id === sessionId)?.kind ?? "independent";

    try {
      if (kind === "group") {
        const [messages, members, fileGrants, mlGrants] = await Promise.all([
          invoke<Message[]>("list_messages", { sessionId }),
          invoke<Agent[]>("list_session_members", { sessionId }),
          invoke<FileAccessGrant[]>("list_session_shared_file_grants", { sessionId }),
          invoke<MlAccessGrant[]>("list_ml_access_grants_for_session", { sessionId }),
        ]);
        patchTab(sessionId, { kind, messages, members, agent: null, fileGrants, mlGrants });
      } else {
        const [messages, agentId] = await Promise.all([
          invoke<Message[]>("list_messages", { sessionId }),
          invoke<string | null>("get_session_agent_id", { sessionId }),
        ]);
        const agent = agents.find((a) => a.id === agentId) ?? null;
        const [fileGrants, skillGrants, mlGrants] = agent
          ? await Promise.all([
              invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: agent.id }),
              invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id }),
              invoke<MlAccessGrant[]>("list_ml_access_grants_for_agent", { agentId: agent.id }),
            ])
          : [[], [], []];
        patchTab(sessionId, { kind, messages, agent, members: [], fileGrants, skillGrants, mlGrants });
      }
    } catch (err) {
      setError(String(err));
    }
  }

  function closeTab(sessionId: string) {
    setOpenTabIds((prev) => prev.filter((id) => id !== sessionId));
    setTabs((prev) => {
      const next = { ...prev };
      delete next[sessionId];
      return next;
    });
    if (activeSessionId === sessionId) {
      const remaining = openTabIds.filter((id) => id !== sessionId);
      setActiveSessionId(remaining[remaining.length - 1] ?? null);
    }
  }

  async function handleCreateSession(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!newSessionAgentId) {
      setError(t("chat.createAnAgentFirst"));
      return;
    }
    try {
      const agent = agents.find((a) => a.id === newSessionAgentId);
      const title = newSessionTitle || t("chat.defaultSessionTitle", { agentName: agent?.name ?? t("chat.defaultAgentName") });
      const session = await invoke<Session>("create_independent_session", {
        title,
        agentId: newSessionAgentId,
      });
      setNewSessionTitle("");
      await refreshSessions();
      await openTab(session.id, "independent");
    } catch (err) {
      setError(String(err));
    }
  }

  function toggleNewGroupAgent(agentId: string) {
    setNewGroupAgentIds((prev) => (prev.includes(agentId) ? prev.filter((id) => id !== agentId) : [...prev, agentId]));
  }

  async function handleCreateGroupSession(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (newGroupAgentIds.length === 0) {
      setError(t("chat.pickAtLeastOneAgent"));
      return;
    }
    try {
      const title = newGroupTitle || t("chat.defaultGroupChatTitle");
      const session = await invoke<Session>("create_group_session", {
        title,
        agentIds: newGroupAgentIds,
      });
      setNewGroupTitle("");
      setNewGroupAgentIds([]);
      setShowNewGroup(false);
      await refreshSessions();
      await openTab(session.id, "group");
    } catch (err) {
      setError(String(err));
    }
  }

  /// Creates a new custom template, or — when `editingTemplateId` is set
  /// — saves changes to that existing one in place instead. Same form,
  /// same handler; only which command gets called differs.
  async function handleSaveTemplate(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      if (editingTemplateId) {
        await invoke("update_custom_role_template", {
          id: editingTemplateId,
          name: templateName,
          description: templateDescription,
          systemPrompt: templatePrompt,
          suggestedProviderKind: null,
          suggestedProviderName: null,
          suggestedModel: null,
        });
      } else {
        await invoke("create_custom_role_template", {
          name: templateName,
          description: templateDescription,
          systemPrompt: templatePrompt,
          suggestedProviderKind: null,
          suggestedProviderName: null,
          suggestedModel: null,
        });
      }
      setTemplateName("");
      setTemplateDescription("");
      setTemplatePrompt("");
      setEditingTemplateId(null);
      setShowNewTemplate(false);
      await refreshRoleTemplates();
    } catch (err) {
      setError(String(err));
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
    setError(null);
    try {
      await invoke("delete_custom_role_template", { id });
      await refreshRoleTemplates();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleExportTemplate(template: RoleTemplate) {
    setError(null);
    try {
      const destPath = await saveFilePicker({
        defaultPath: `${template.name.replace(/[^a-zA-Z0-9 _-]/g, "_")}.json`,
        filters: [{ name: "Role Template", extensions: ["json"] }],
      });
      if (!destPath) return; // user cancelled the picker
      await invoke("export_custom_role_template", { id: template.id, destPath });
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleImportTemplate() {
    setError(null);
    try {
      const sourcePath = await openFolderPicker({
        directory: false,
        multiple: false,
        filters: [{ name: "Role Template", extensions: ["json"] }],
      });
      if (!sourcePath) return; // user cancelled the picker
      await invoke("import_custom_role_template", { sourcePath });
      await refreshRoleTemplates();
    } catch (err) {
      setError(String(err));
    }
  }

  /// Independent Session tabs grant a private folder to their one agent;
  /// Group Chat tabs grant a folder shared by every member currently in
  /// the meeting (`grant_folder_access_for_session`) — same real OS
  /// folder picker either way, only the resulting scope differs.
  async function handleGrantFolder(sessionId: string) {
    const tab = tabs[sessionId];
    if (!tab) return;
    const grantingAgentId = tab.kind === "group" ? tab.members[0]?.id : tab.agent?.id;
    if (!grantingAgentId) return;
    setError(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return; // user cancelled the picker
      if (tab.kind === "group") {
        await invoke("grant_folder_access_for_session", { sessionId, agentId: grantingAgentId, folderPath: folder });
        const fileGrants = await invoke<FileAccessGrant[]>("list_session_shared_file_grants", { sessionId });
        patchTab(sessionId, { fileGrants });
      } else {
        await invoke("grant_folder_access", { agentId: grantingAgentId, folderPath: folder });
        const fileGrants = await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: grantingAgentId });
        patchTab(sessionId, { fileGrants });
      }
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeGrant(sessionId: string, id: string) {
    const tab = tabs[sessionId];
    if (!tab) return;
    setError(null);
    try {
      await invoke("revoke_file_access_grant", { id });
      if (tab.kind === "group") {
        const fileGrants = await invoke<FileAccessGrant[]>("list_session_shared_file_grants", { sessionId });
        patchTab(sessionId, { fileGrants });
      } else if (tab.agent) {
        const fileGrants = await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: tab.agent.id });
        patchTab(sessionId, { fileGrants });
      }
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleGrantSkill(sessionId: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent || !skillToGrant) return;
    setError(null);
    try {
      await invoke("grant_skill_access", { agentId: agent.id, skillName: skillToGrant });
      const skillGrants = await invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id });
      patchTab(sessionId, { skillGrants });
      setSkillToGrant("");
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeSkill(sessionId: string, id: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent) return;
    setError(null);
    try {
      await invoke("revoke_skill_access", { id });
      const skillGrants = await invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id });
      patchTab(sessionId, { skillGrants });
    } catch (err) {
      setError(String(err));
    }
  }

  /// Runs `runSkillName` with the JSON typed into `runSkillPayload` on
  /// `sessionId`'s agent, and drops the result into the transcript as a
  /// `role: "system"` message — visible, but not attributed to "user" or
  /// "assistant". Goes through the same Guardrails injection screen +
  /// per-agent allowlist as every other Skill call.
  async function handleRunSkill(sessionId: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent || !runSkillName) return;
    setError(null);
    let payload: unknown;
    try {
      payload = JSON.parse(runSkillPayload || "{}");
    } catch {
      setError(t("chat.skillPayloadMustBeJson"));
      return;
    }
    setRunningSkill(true);
    try {
      await invoke("run_skill_in_session", { sessionId, agentId: agent.id, skillName: runSkillName, payload });
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      patchTab(sessionId, { messages });
    } catch (err) {
      setError(String(err));
    } finally {
      setRunningSkill(false);
    }
  }

  /// Imports a user-picked folder (containing `skill.json` + its
  /// entrypoint) as a new custom Skill, global to the whole app catalog
  /// (not per-session, unlike grants) — refreshes `availableSkills` so it
  /// shows up immediately in every session's "Grant a skill…" picker.
  async function handleImportSkill() {
    setError(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return; // user cancelled the picker
      setImportingSkill(true);
      await invoke("import_custom_skill", { sourceFolder: folder });
      setAvailableSkills(await invoke<SkillManifest[]>("list_skills"));
    } catch (err) {
      setError(String(err));
    } finally {
      setImportingSkill(false);
    }
  }

  /// Semantic search treats Group Chat as a whole differently from an
  /// Independent Session's single agent: grants and the index itself are
  /// scoped to the *session* (shared by every current member — the same
  /// "同場會議共用" rule File Access just got, see `ML Engine Design.md`
  /// §4.1), not to whichever agent happens to be acting. `actingAgentId`
  /// is only needed because the Tauri commands still take an agent id to
  /// resolve File Access grants through (`effective_granted_folders`
  /// already includes the session's shared folders for any member) —
  /// it doesn't change *whose* access is being granted or *which* index
  /// is being built/queried.
  function mlScopeFor(sessionId: string): { actingAgentId: string; indexName: string } | null {
    const tab = tabs[sessionId];
    if (!tab) return null;
    if (tab.kind === "group") {
      const actingAgentId = tab.members[0]?.id;
      return actingAgentId ? { actingAgentId, indexName: `group-${sessionId}` } : null;
    }
    return tab.agent ? { actingAgentId: tab.agent.id, indexName: tab.agent.id } : null;
  }

  async function handleGrantMlCapability(sessionId: string) {
    const tab = tabs[sessionId];
    const scope = mlScopeFor(sessionId);
    if (!tab || !scope || !mlCapabilityToGrant) return;
    setError(null);
    try {
      if (tab.kind === "group") {
        await invoke("grant_ml_capability_to_session", { sessionId, capabilityName: mlCapabilityToGrant });
        const mlGrants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_session", { sessionId });
        patchTab(sessionId, { mlGrants });
      } else {
        await invoke("grant_ml_capability_to_agent", { agentId: scope.actingAgentId, capabilityName: mlCapabilityToGrant });
        const mlGrants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_agent", { agentId: scope.actingAgentId });
        patchTab(sessionId, { mlGrants });
      }
      setMlCapabilityToGrant("");
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeMlCapability(sessionId: string, id: string) {
    const tab = tabs[sessionId];
    const scope = mlScopeFor(sessionId);
    if (!tab || !scope) return;
    setError(null);
    try {
      await invoke("revoke_ml_access_grant", { id });
      if (tab.kind === "group") {
        const mlGrants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_session", { sessionId });
        patchTab(sessionId, { mlGrants });
      } else {
        const mlGrants = await invoke<MlAccessGrant[]>("list_ml_access_grants_for_agent", { agentId: scope.actingAgentId });
        patchTab(sessionId, { mlGrants });
      }
    } catch (err) {
      setError(String(err));
    }
  }

  /// Rebuilds the search index — the agent's own granted folders for an
  /// Independent Session, or the whole meeting's shared folders for a
  /// Group Chat (`group-<sessionId>`, per `ML Engine Design.md` §4.1).
  /// Deliberately two different Tauri commands, not one with a flag:
  /// `build_semantic_index_for_session` sources files from *only* what
  /// was shared to the session (`list_session_shared_file_grants`), never
  /// the acting member's own private grants — using the private-scoped
  /// command here would leak that member's private files into an index
  /// every meeting participant can search.
  async function handleBuildIndex(sessionId: string) {
    const tab = tabs[sessionId];
    const scope = mlScopeFor(sessionId);
    if (!tab || !scope) return;
    setError(null);
    setIndexing(true);
    try {
      if (tab.kind === "group") {
        await invoke("build_semantic_index_for_session", {
          sessionId,
          agentId: scope.actingAgentId,
          indexName: scope.indexName,
        });
      } else {
        await invoke("build_semantic_index", { agentId: scope.actingAgentId, indexName: scope.indexName });
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setIndexing(false);
    }
  }

  async function handleSemanticSearch(sessionId: string) {
    const scope = mlScopeFor(sessionId);
    if (!scope || !searchQuery.trim()) return;
    setError(null);
    setSearching(true);
    try {
      const result = await invoke<{ results: SemanticSearchResult[] }>("semantic_search_query", {
        agentId: scope.actingAgentId,
        indexName: scope.indexName,
        query: searchQuery,
        topK: 5,
      });
      patchTab(sessionId, { searchResults: result.results });
    } catch (err) {
      setError(String(err));
    } finally {
      setSearching(false);
    }
  }

  /// Sends whatever `sessionId`'s draft is. Deliberately keyed off the
  /// session, not "the active session" — this is what lets the user
  /// switch to a different tab while this call is still in flight and
  /// keep working there; the reply lands in the right tab whenever it
  /// arrives, active or not.
  async function sendForSession(sessionId: string) {
    const tab = tabs[sessionId];
    if (!tab || !tab.draft.trim() || tab.sending) return;
    setError(null);
    const content = tab.draft;
    patchTab(sessionId, { draft: "", sending: true });
    try {
      if (tab.kind === "group") {
        const result = await invoke<GroupTurnResult>("send_group_message", { sessionId, content });
        if (result.kind === "boundaryConfirmationNeeded") {
          patchTab(sessionId, {
            sending: false,
            pendingBoundary: result.previewContent,
            pendingBoundaryRetry: "turn",
          });
          return;
        }
      } else {
        await invoke("send_chat_message", { sessionId, content });
      }
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      setTabs((prev) => ({
        ...prev,
        [sessionId]: {
          ...(prev[sessionId] ?? emptyTab()),
          messages,
          sending: false,
          hasUnseenReply: sessionId !== activeSessionId,
        },
      }));
    } catch (err) {
      setError(String(err));
      patchTab(sessionId, { sending: false });
    }
  }

  function handleSend(e: FormEvent, sessionId: string) {
    e.preventDefault();
    void sendForSession(sessionId);
  }

  /// "Let them keep talking" — one more agent turn in rotation with no
  /// new user message. This is the path Guardrails' loop safety-net
  /// (E6001) actually guards against, since nothing else stops the user
  /// from clicking this repeatedly.
  async function handleAdvanceTurn(sessionId: string) {
    setError(null);
    patchTab(sessionId, { sending: true });
    try {
      const result = await invoke<GroupTurnResult>("advance_group_turn", { sessionId });
      if (result.kind === "boundaryConfirmationNeeded") {
        patchTab(sessionId, {
          sending: false,
          pendingBoundary: result.previewContent,
          pendingBoundaryRetry: "turn",
        });
        return;
      }
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionId });
    } catch (err) {
      setError(String(err));
      patchTab(sessionId, { sending: false });
    }
  }

  /// Runs up to `turns` consecutive `advance_group_turn` calls, one at a
  /// time (awaiting + refreshing messages after each so replies appear
  /// progressively rather than all at once). Deliberately does not add
  /// any new client-side cap of its own — the backend's E6001 loop
  /// safety-net (`orchestrator::MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT`)
  /// is what actually stops this from running away; hitting it here just
  /// surfaces as the normal error banner and ends the loop early, same as
  /// if the user had clicked "Let them continue" that many times by hand.
  /// Also stops early (without an error) if a turn pauses for the
  /// local→cloud boundary confirmation (E6004) — the user needs to decide
  /// before any further turns run.
  async function handleAutoContinue(sessionId: string, turns: number) {
    setError(null);
    for (let i = 0; i < turns; i++) {
      patchTab(sessionId, { sending: true });
      try {
        const result = await invoke<GroupTurnResult>("advance_group_turn", { sessionId });
        if (result.kind === "boundaryConfirmationNeeded") {
          patchTab(sessionId, {
            sending: false,
            pendingBoundary: result.previewContent,
            pendingBoundaryRetry: "turn",
          });
          break;
        }
        const messages = await invoke<Message[]>("list_messages", { sessionId });
        patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionId });
      } catch (err) {
        setError(String(err));
        patchTab(sessionId, { sending: false });
        break;
      }
    }
  }

  /// User approved sending the previewed local-Agent content to a cloud
  /// provider (E6004). Grants session-scoped consent, then retries
  /// whichever action paused — a regular turn (`advance_group_turn`,
  /// not `send_group_message`, since the user's message was already
  /// persisted before the pause) or ending the meeting
  /// (`end_group_chat_meeting`, which can also hit this boundary via its
  /// summarizer).
  async function handleConfirmBoundary(sessionId: string) {
    setError(null);
    const retry = tabs[sessionId]?.pendingBoundaryRetry ?? "turn";
    patchTab(sessionId, { pendingBoundary: null, pendingBoundaryRetry: null, sending: true });
    try {
      await invoke("confirm_local_to_cloud_boundary", { sessionId });
      const result =
        retry === "endMeeting"
          ? await invoke<GroupTurnResult>("end_group_chat_meeting", { sessionId, summarizerAgentId: null })
          : await invoke<GroupTurnResult>("advance_group_turn", { sessionId });
      if (result.kind === "boundaryConfirmationNeeded") {
        patchTab(sessionId, { sending: false, pendingBoundary: result.previewContent, pendingBoundaryRetry: retry });
        return;
      }
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionId });
    } catch (err) {
      setError(String(err));
      patchTab(sessionId, { sending: false });
    }
  }

  /// User declined — just dismiss the prompt, no consent granted, no
  /// turn advanced. The paused turn can be retried later (e.g. via
  /// "Let them continue").
  function handleCancelBoundary(sessionId: string) {
    patchTab(sessionId, { pendingBoundary: null, pendingBoundaryRetry: null });
  }

  /// Ends the meeting: the backend picks a summarizer (Product Lead
  /// member, else whoever joined first) and asks them to wrap up.
  async function handleEndMeeting(sessionId: string) {
    setError(null);
    patchTab(sessionId, { sending: true });
    try {
      const result = await invoke<GroupTurnResult>("end_group_chat_meeting", {
        sessionId,
        summarizerAgentId: null,
      });
      if (result.kind === "boundaryConfirmationNeeded") {
        patchTab(sessionId, {
          sending: false,
          pendingBoundary: result.previewContent,
          pendingBoundaryRetry: "endMeeting",
        });
        return;
      }
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      patchTab(sessionId, { messages, sending: false });
    } catch (err) {
      setError(String(err));
      patchTab(sessionId, { sending: false });
    }
  }

  /// Shared by both Independent Session and Group Chat headers — the
  /// grant/revoke chips, rebuild-index + query form, and results list
  /// are identical either way; only what `mlScopeFor` resolves them to
  /// (one agent vs. the whole session) differs.
  function renderSemanticSearchSection(sessionId: string, tab: TabState) {
    return (
      <>
        <div className="chat-file-access">
          <span>{t("chat.semanticSearchLabel")}</span>
          {tab.mlGrants.length === 0 && <span className="chat-empty">{t("chat.notGranted")}</span>}
          {tab.mlGrants.map((g) => (
            <span key={g.id} className="chat-file-chip">
              {g.capabilityName}
              <button
                onClick={() => handleRevokeMlCapability(sessionId, g.id)}
                title={t("chat.revokeAccess")}
                aria-label={t("chat.revokeAccessToCapability", { capabilityName: g.capabilityName })}
              >
                ×
              </button>
            </span>
          ))}
          <select value={mlCapabilityToGrant} onChange={(e) => setMlCapabilityToGrant(e.target.value)}>
            <option value="">{t("chat.grantMlCapability")}</option>
            {availableMlCapabilities
              .filter((c) => !tab.mlGrants.some((g) => g.capabilityName === c.name))
              .map((c) => (
                <option key={c.name} value={c.name}>
                  {c.name}
                </option>
              ))}
          </select>
          <button className="chat-link-button" disabled={!mlCapabilityToGrant} onClick={() => handleGrantMlCapability(sessionId)}>
            {t("chat.grant")}
          </button>
        </div>
        {tab.mlGrants.some((g) => g.capabilityName === "semantic_search") && (
          <div className="chat-run-skill">
            <button
              className="chat-link-button"
              disabled={indexing}
              onClick={() => handleBuildIndex(sessionId)}
              title={t("chat.rebuildIndexTitle")}
            >
              {indexing ? t("chat.indexing") : t("chat.rebuildIndex")}
            </button>
            <input
              type="text"
              placeholder={t("chat.searchFilesPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            <button
              className="chat-link-button"
              disabled={!searchQuery.trim() || searching}
              onClick={() => handleSemanticSearch(sessionId)}
            >
              {searching ? t("chat.searching") : t("chat.search")}
            </button>
          </div>
        )}
        {tab.searchResults && (
          <ul className="chat-search-results">
            {tab.searchResults.length === 0 && <li className="chat-empty">{t("chat.noResults")}</li>}
            {tab.searchResults.map((r) => (
              <li key={r.path}>
                <div className="chat-search-result-head">
                  <span className="chat-search-result-path">{r.path}</span>
                  <span className="chat-search-result-score">{r.score.toFixed(3)}</span>
                </div>
                <div className="chat-search-result-excerpt">{r.excerpt}</div>
              </li>
            ))}
          </ul>
        )}
      </>
    );
  }

  return (
    <div className="chat-page">
      <aside className="chat-sidebar">
        <h2>{t("chat.independentSessions")}</h2>
        <ul className="chat-session-list">
          {independentSessions.map((s) => (
            <li key={s.id}>
              <button className={openTabIds.includes(s.id) ? "active" : ""} onClick={() => openTab(s.id, s.kind)}>
                {s.title}
                {tabs[s.id]?.hasUnseenReply && <span className="chat-unread-dot" />}
              </button>
            </li>
          ))}
          {independentSessions.length === 0 && <li className="chat-empty">{t("chat.noSessions")}</li>}
        </ul>

        <h2>{t("chat.groupChats")}</h2>
        <ul className="chat-session-list">
          {groupSessions.map((s) => (
            <li key={s.id}>
              <button className={openTabIds.includes(s.id) ? "active" : ""} onClick={() => openTab(s.id, s.kind)}>
                {s.title}
                {tabs[s.id]?.hasUnseenReply && <span className="chat-unread-dot" />}
              </button>
            </li>
          ))}
          {groupSessions.length === 0 && <li className="chat-empty">{t("chat.noGroupChats")}</li>}
        </ul>

        <button className="chat-link-button" onClick={() => setShowNewGroup((v) => !v)}>
          {showNewGroup ? t("chat.cancel") : t("chat.newGroupChat")}
        </button>
        {showNewGroup && (
          <form className="chat-form" onSubmit={handleCreateGroupSession}>
            <input
              type="text"
              placeholder={t("chat.meetingTitlePlaceholder")}
              value={newGroupTitle}
              onChange={(e) => setNewGroupTitle(e.target.value)}
            />
            <div className="chat-group-agent-picker">
              {agents.length === 0 && <span className="chat-empty">{t("chat.noAgentsYet")}</span>}
              {agents.map((a) => (
                <label key={a.id} className="chat-group-agent-option">
                  <input
                    type="checkbox"
                    checked={newGroupAgentIds.includes(a.id)}
                    onChange={() => toggleNewGroupAgent(a.id)}
                  />
                  {a.name}
                </label>
              ))}
            </div>
            <button type="submit" disabled={agents.length === 0}>
              {t("chat.startMeeting")}
            </button>
          </form>
        )}

        <h3>{t("chat.newSession")}</h3>
        <form className="chat-form" onSubmit={handleCreateSession}>
          <select value={newSessionAgentId} onChange={(e) => setNewSessionAgentId(e.target.value)}>
            {agents.length === 0 && <option value="">{t("chat.noAgentsYetOption")}</option>}
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name} ({a.providerName}/{a.model})
              </option>
            ))}
          </select>
          <input
            type="text"
            placeholder={t("chat.sessionTitlePlaceholder")}
            value={newSessionTitle}
            onChange={(e) => setNewSessionTitle(e.target.value)}
          />
          <button type="submit" disabled={agents.length === 0}>
            {t("chat.start")}
          </button>
        </form>

        <button
          className="chat-link-button"
          onClick={() => {
            setShowNewAgent((v) => !v);
            setNewAgentProviderTouched(false);
          }}
        >
          {showNewAgent ? t("chat.cancel") : t("chat.newAgent")}
        </button>
        {showNewAgent && (
          <form className="chat-form" onSubmit={handleCreateAgent}>
            <select value={newAgentTemplateId} onChange={(e) => handleSelectTemplate(e.target.value)}>
              <option value="">{t("chat.noRoleTemplate")}</option>
              {roleTemplates.map((rt) => (
                <option key={rt.id} value={rt.id}>
                  {rt.name} {rt.source === "custom" ? t("chat.custom") : ""}
                </option>
              ))}
            </select>
            <input
              type="text"
              placeholder={t("chat.agentNamePlaceholder")}
              value={newAgentName}
              onChange={(e) => setNewAgentName(e.target.value)}
              required
            />
            <select
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
            {(() => {
              const selectedTemplate = roleTemplates.find((rt) => rt.id === newAgentTemplateId);
              const suggested = selectedTemplate?.suggestedProviderName;
              if (!suggested || suggested === newAgentProvider) return null;
              return (
                <button type="button" className="chat-link-button" onClick={handleApplyTemplateSuggestion}>
                  {t("chat.applySuggestedProvider", { provider: suggested })}
                </button>
              );
            })()}
            <select value={newAgentModel} onChange={(e) => setNewAgentModel(e.target.value)}>
              {newAgentModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
            <textarea
              rows={3}
              placeholder={t("chat.systemPromptPlaceholder")}
              value={newAgentSystemPrompt}
              onChange={(e) => setNewAgentSystemPrompt(e.target.value)}
            />
            {!isLocalProvider(newAgentProvider) && (
              <select value={newAgentPinnedKeyId} onChange={(e) => setNewAgentPinnedKeyId(e.target.value)}>
                <option value="">{t("chat.useLatestKeyDefault", { provider: newAgentProvider })}</option>
                {newAgentProviderKeys.map((k) => (
                  <option key={k.id} value={k.id}>
                    {t("chat.pinTo", { label: k.label ?? k.maskedSecret })}
                  </option>
                ))}
              </select>
            )}

            <div className="chat-fallback-chain">
              <span>{t("chat.fallbackLabel")}</span>
              {fallbackChain.length === 0 && <span className="chat-empty">{t("chat.noneConfigured")}</span>}
              {fallbackChain.map((step, i) => (
                <span key={i} className="chat-file-chip">
                  {i + 1}. {step.providerName}/{step.model}
                  <button
                    type="button"
                    onClick={() => handleRemoveFallbackStep(i)}
                    title={t("chat.remove")}
                    aria-label={t("chat.removeFallbackStep", { step: `${step.providerName}/${step.model}` })}
                  >
                    ×
                  </button>
                </span>
              ))}
              <div className="acc-form-row">
                <select value={fallbackProvider} onChange={(e) => setFallbackProvider(e.target.value)}>
                  {PROVIDER_OPTIONS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                <input
                  type="text"
                  placeholder={t("chat.modelIdPlaceholder")}
                  value={fallbackModel}
                  onChange={(e) => setFallbackModel(e.target.value)}
                />
                <button type="button" disabled={!fallbackModel.trim()} onClick={() => handleAddFallbackStep()}>
                  {t("chat.addFallback")}
                </button>
              </div>
            </div>

            <button type="submit">{t("chat.createAgent")}</button>
          </form>
        )}

        <h3>{t("chat.customRoleTemplates")}</h3>
        <ul className="chat-session-list">
          {roleTemplates
            .filter((rt) => rt.source === "custom")
            .map((rt) => (
              <li key={rt.id} className="chat-template-row">
                <span title={rt.description}>{rt.name}</span>
                <span className="chat-template-row-actions">
                  <button className="chat-link-button" onClick={() => handleStartEditTemplate(rt)}>
                    {t("chat.edit")}
                  </button>
                  <button className="chat-link-button" onClick={() => handleExportTemplate(rt)}>
                    {t("chat.export")}
                  </button>
                  <button className="chat-link-button" onClick={() => handleDeleteTemplate(rt.id)}>
                    {t("chat.delete")}
                  </button>
                </span>
              </li>
            ))}
          {roleTemplates.filter((rt) => rt.source === "custom").length === 0 && (
            <li className="chat-empty">{t("chat.noCustomTemplates")}</li>
          )}
        </ul>

        <button
          className="chat-link-button"
          onClick={() => (showNewTemplate ? handleCancelTemplateForm() : setShowNewTemplate(true))}
        >
          {showNewTemplate ? t("chat.cancel") : t("chat.newRoleTemplate")}
        </button>
        <button className="chat-link-button" onClick={() => handleImportTemplate()}>
          {t("chat.importTemplate")}
        </button>
        {showNewTemplate && (
          <form className="chat-form" onSubmit={handleSaveTemplate}>
            <input
              type="text"
              placeholder={t("chat.templateNamePlaceholder")}
              value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder={t("chat.shortDescriptionPlaceholder")}
              value={templateDescription}
              onChange={(e) => setTemplateDescription(e.target.value)}
              required
            />
            <textarea
              rows={3}
              placeholder={t("chat.systemPromptPlainPlaceholder")}
              value={templatePrompt}
              onChange={(e) => setTemplatePrompt(e.target.value)}
              required
            />
            <button type="submit">{editingTemplateId ? t("chat.saveChanges") : t("chat.saveTemplate")}</button>
          </form>
        )}
      </aside>

      <main className="chat-main">
        {error &&
          (() => {
            const { code, rest } = parseErrorCode(error);
            return (
              <div className="chat-error" role="alert">
                <div className="chat-error-body">
                  {code && <span className="chat-error-code">{code}</span>}
                  <span>{rest}</span>
                </div>
                <div className="chat-error-actions">
                  {code && (
                    <button
                      className="chat-error-copy"
                      onClick={() => void navigator.clipboard.writeText(error)}
                    >
                      {t("chat.copyErrorDetails")}
                    </button>
                  )}
                  <button onClick={() => setError(null)} aria-label={t("chat.dismissError")}>×</button>
                </div>
              </div>
            );
          })()}

        {openTabIds.length > 0 && (
          <div className="chat-tabs">
            {openTabIds.map((id) => {
              const title = sessions.find((s) => s.id === id)?.title ?? "…";
              const tab = tabs[id];
              return (
                <div key={id} className={`chat-tab ${id === activeSessionId ? "active" : ""}`}>
                  <button className="chat-tab-select" onClick={() => setActiveSessionId(id)}>
                    {title}
                    {tab?.sending && <span className="chat-tab-spinner" title="Waiting for reply…" />}
                    {tab?.hasUnseenReply && <span className="chat-unread-dot" />}
                  </button>
                  <button
                    className="chat-tab-close"
                    onClick={() => closeTab(id)}
                    title={t("chat.closeTabTitle")}
                    aria-label={t("chat.closeTab", { title })}
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
        )}

        {!activeSessionId && <p className="chat-empty">{t("chat.pickOrStartSession")}</p>}

        {activeSessionId && activeTab && (
          <>
            {activeTab.agent && (
              <div className="chat-header">
                <div>
                  {t("chat.chattingWith")} <strong>{activeTab.agent.name}</strong> ({activeTab.agent.providerName}/
                  {activeTab.agent.model})
                </div>
                <div className="chat-file-access">
                  <span>{t("chat.files")}</span>
                  {activeTab.fileGrants.length === 0 && (
                    <span className="chat-empty">{t("chat.noFoldersGranted")}</span>
                  )}
                  {activeTab.fileGrants.map((g) => (
                    <span key={g.id} className="chat-file-chip">
                      {g.folderPath}
                      <button
                        onClick={() => handleRevokeGrant(activeSessionId, g.id)}
                        title={t("chat.revokeAccess")}
                        aria-label={t("chat.revokeAccessToFolder", { folderPath: g.folderPath })}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <button className="chat-link-button" onClick={() => handleGrantFolder(activeSessionId)}>
                    {t("chat.grantFolder")}
                  </button>
                </div>
                <div className="chat-file-access">
                  <span>{t("chat.skills")}</span>
                  {activeTab.skillGrants.length === 0 && (
                    <span className="chat-empty">{t("chat.noneGranted")}</span>
                  )}
                  {activeTab.skillGrants.map((g) => (
                    <span key={g.id} className="chat-file-chip">
                      {g.skillName}
                      <button
                        onClick={() => handleRevokeSkill(activeSessionId, g.id)}
                        title={t("chat.revokeAccess")}
                        aria-label={t("chat.revokeAccessToSkill", { skillName: g.skillName })}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <select value={skillToGrant} onChange={(e) => setSkillToGrant(e.target.value)}>
                    <option value="">{t("chat.grantASkill")}</option>
                    {availableSkills
                      .filter((s) => !activeTab.skillGrants.some((g) => g.skillName === s.name))
                      .map((s) => (
                        <option key={s.name} value={s.name}>
                          {s.name} {s.source === "custom" ? t("chat.custom") : ""}
                        </option>
                      ))}
                  </select>
                  <button className="chat-link-button" disabled={!skillToGrant} onClick={() => handleGrantSkill(activeSessionId)}>
                    {t("chat.grant")}
                  </button>
                  <button className="chat-link-button" disabled={importingSkill} onClick={() => handleImportSkill()}>
                    {importingSkill ? t("chat.importingSkill") : t("chat.importCustomSkill")}
                  </button>
                </div>
                <p className="chat-skill-import-warning">{t("chat.skillImportWarning")}</p>
                {activeTab.skillGrants.length > 0 && (
                  <div className="chat-run-skill">
                    <select value={runSkillName} onChange={(e) => setRunSkillName(e.target.value)}>
                      <option value="">{t("chat.runASkill")}</option>
                      {activeTab.skillGrants.map((g) => (
                        <option key={g.id} value={g.skillName}>
                          {g.skillName}
                        </option>
                      ))}
                    </select>
                    <input
                      type="text"
                      placeholder={t("chat.jsonPayloadPlaceholder")}
                      value={runSkillPayload}
                      onChange={(e) => setRunSkillPayload(e.target.value)}
                    />
                    <button
                      className="chat-link-button"
                      disabled={!runSkillName || runningSkill}
                      onClick={() => handleRunSkill(activeSessionId)}
                    >
                      {runningSkill ? t("chat.running") : t("chat.run")}
                    </button>
                  </div>
                )}
                {renderSemanticSearchSection(activeSessionId, activeTab)}
              </div>
            )}
            {activeTab.kind === "group" && (
              <div className="chat-header">
                <div>
                  {t("chat.groupChatLabel", { members: activeTab.members.map((m) => m.name).join(", ") || t("chat.noMembers") })}
                </div>
                <div className="chat-group-actions">
                  <button
                    className="chat-link-button"
                    disabled={activeTab.sending}
                    onClick={() => handleAdvanceTurn(activeSessionId)}
                  >
                    {t("chat.letThemContinue")}
                  </button>
                  <input
                    type="number"
                    min={1}
                    max={6}
                    value={autoContinueTurns}
                    disabled={activeTab.sending}
                    onChange={(e) => setAutoContinueTurns(Math.max(1, Math.min(6, Number(e.target.value) || 1)))}
                    className="chat-auto-continue-count"
                    title={t("chat.autoContinueTitle")}
                  />
                  <button
                    className="chat-link-button"
                    disabled={activeTab.sending}
                    onClick={() => handleAutoContinue(activeSessionId, autoContinueTurns)}
                    title={t("chat.continueTurnsTitle")}
                  >
                    {t("chat.continueTurns", { turns: autoContinueTurns })}
                  </button>
                  <button
                    className="chat-link-button"
                    disabled={activeTab.sending}
                    onClick={() => handleEndMeeting(activeSessionId)}
                  >
                    {t("chat.endMeeting")}
                  </button>
                </div>
                {activeTab.pendingBoundary && (
                  <div className="chat-boundary-confirm">
                    <div>
                      <strong>{t("chat.boundaryConfirmTitle")}</strong> {t("chat.boundaryConfirmBody")}
                    </div>
                    <pre className="chat-boundary-preview">{activeTab.pendingBoundary}</pre>
                    <div className="chat-group-actions">
                      <button
                        className="chat-link-button"
                        onClick={() => handleConfirmBoundary(activeSessionId)}
                      >
                        {t("chat.sendToCloud")}
                      </button>
                      <button
                        className="chat-link-button"
                        onClick={() => handleCancelBoundary(activeSessionId)}
                      >
                        {t("chat.cancel")}
                      </button>
                    </div>
                  </div>
                )}
                <div className="chat-file-access">
                  <span>{t("chat.filesSharedWithMeeting")}</span>
                  {activeTab.fileGrants.length === 0 && (
                    <span className="chat-empty">{t("chat.noFoldersGranted")}</span>
                  )}
                  {activeTab.fileGrants.map((g) => (
                    <span key={g.id} className="chat-file-chip">
                      {g.folderPath}
                      <button
                        onClick={() => handleRevokeGrant(activeSessionId, g.id)}
                        title={t("chat.revokeAccess")}
                        aria-label={t("chat.revokeAccessToFolder", { folderPath: g.folderPath })}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <button
                    className="chat-link-button"
                    disabled={activeTab.members.length === 0}
                    onClick={() => handleGrantFolder(activeSessionId)}
                  >
                    {t("chat.grantFolder")}
                  </button>
                </div>
                {renderSemanticSearchSection(activeSessionId, activeTab)}
              </div>
            )}
            <div className="chat-thread">
              {activeTab.messages.length === 0 && (
                <p className="chat-empty">{t("chat.noMessagesYet")}</p>
              )}
              {activeTab.messages.map((m) => {
                const speakerName =
                  activeTab.kind === "group" && m.agentId
                    ? activeTab.members.find((mem) => mem.id === m.agentId)?.name ?? m.role
                    : m.role;
                return (
                  <div key={m.id} className={`chat-bubble chat-bubble-${m.role}`}>
                    <div className="chat-bubble-role">{speakerName}</div>
                    <div className="chat-bubble-content">{m.content}</div>
                  </div>
                );
              })}
              {activeTab.sending && (
                <div className="chat-bubble chat-bubble-assistant chat-bubble-pending">
                  <div className="chat-bubble-role">assistant</div>
                  <div className="chat-bubble-content">…</div>
                </div>
              )}
              <div ref={bottomRef} />
            </div>
            <form className="chat-input-row" onSubmit={(e) => handleSend(e, activeSessionId)}>
              <input
                type="text"
                placeholder={
                  activeTab.kind === "group"
                    ? t("chat.messagePlaceholderGroup")
                    : t("chat.messagePlaceholderIndependent")
                }
                value={activeTab.draft}
                onChange={(e) => patchTab(activeSessionId, { draft: e.target.value })}
              />
              <button type="submit" disabled={!activeTab.draft.trim()}>
                {activeTab.sending ? t("chat.sending") : t("chat.send")}
              </button>
            </form>
          </>
        )}
      </main>
    </div>
  );
}
