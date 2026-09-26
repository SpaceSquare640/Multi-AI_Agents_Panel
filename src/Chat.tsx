import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "./shell/Icons";
import { ShellInspector, ShellSidebar, useIsActiveScreen } from "./shell/SidebarSlot";
import { invoke } from "@tauri-apps/api/core";
import { ask, open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import type {
  Agent,
  AgentMemory,
  FileAccessGrant,
  GroupTurnResult,
  McpAccessGrant,
  McpServer,
  McpTool,
  Message,
  MlAccessGrant,
  MlCapabilityManifest,
  SemanticSearchResult,
  Session,
  SkillAccessGrant,
  SkillManifest,
} from "./types";
import "./styles/screens/agents.css";
import "./styles/screens/chat.css";
import "./styles/screens/chat-stream.css";
import "./styles/screens/inspector.css";
import "./Chat.css";

/// The providers `send_chat_message_with_tools`
/// (agent_manager::function_calling) implements a tool-calling protocol
/// for. Local providers are deliberately absent: most local models do not
/// reliably support structured tool calling, and a model that silently
/// ignores the tools answers as if it had run the Skill when it never
/// did. Keep this in step with `Wire::for_provider` on the Rust side —
/// the backend is the authority, and anything listed here that it does
/// not know returns "Unsupported" at send time.
const FUNCTION_CALLING_PROVIDERS = ["anthropic", "openai", "openrouter"];

/// What a stopped send comes back as, matching `commands::CANCELLED` on
/// the Rust side. Kept as a named constant on both sides because the two
/// have to agree exactly: if they drift, a send the user stopped starts
/// showing up as a red error banner.
const CANCELLED = "cancelled";

/// Mirrors the backend restriction in the UI so the toggle only appears
/// where it would actually work, rather than offering it everywhere and
/// surfacing "Unsupported" after the user has already sent a message.
function functionCallingEligible(tab: TabState): boolean {
  return (
    tab.kind === "independent" &&
    !!tab.agent &&
    FUNCTION_CALLING_PROVIDERS.includes(tab.agent.providerName) &&
    tab.skillGrants.length > 0
  );
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
  /** MCP servers this tab's agent may call tools on. Same "independent
   *  sessions only for now" scope as skillGrants — see mcp_manager
   *  module docs for what per-agent authorization covers (a whole
   *  server, not individual tools within it). */
  mcpGrants: McpAccessGrant[];
  /** ML capabilities (e.g. semantic_search) this tab's agent may call.
   *  Independent Sessions only for now — Group Chat semantic search needs
   *  File Access sharing to land first (see Backlog). */
  mlGrants: MlAccessGrant[];
  /** This tab's agent's long-term memory notes (agent_manager::memory)
   *  — survive across sessions, unlike `messages`. Empty for group tabs
   *  (each member has their own memories; there's no single "the tab's
   *  agent" to show here). */
  memories: AgentMemory[];
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
  /** Whether the next message in this tab should be sent through
   *  `send_chat_message_with_tools` (agent_manager::function_calling)
   *  instead of the plain `send_chat_message`. Only meaningful — and
   *  only shown in the UI — for an independent session whose agent is on
   *  a provider with a tool-calling protocol, with at least one granted
   *  Skill (see `functionCallingEligible` below). */
  useFunctionCalling: boolean;
}

function emptyTab(): TabState {
  return {
    kind: "independent",
    messages: [],
    agent: null,
    members: [],
    fileGrants: [],
    skillGrants: [],
    mcpGrants: [],
    mlGrants: [],
    memories: [],
    useFunctionCalling: false,
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

/** Initials for a speaker's monogram. The design's avatar is a monogram
 *  rather than a picture, and the initials are always present: the colour
 *  is a shortcut for recognising a speaker, never the only way to tell
 *  who is talking. */
export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

/** Which of the design's six agent hues a message's speaker gets.
 *
 *  Assigned by position in the group's member list, so everyone in one
 *  meeting is a different colour and each keeps the same colour for as
 *  long as the meeting does. A solo session has one agent and always
 *  takes the first hue. The user's own messages are their own slot, not
 *  one of the six.
 *
 *  Position rather than a hash of the id: a hash spreads evenly across
 *  runs but says nothing within one, and two of six colliding in the same
 *  meeting is exactly the case the colour exists to prevent. */
export function avatarSlot(
  message: { role: string; agentId: string | null },
  tab: { kind: string; members: { id: string }[] },
): string {
  if (message.role === "user") return "user";
  if (tab.kind !== "group" || !message.agentId) return "1";
  const index = tab.members.findIndex((m) => m.id === message.agentId);
  return String((index < 0 ? 0 : index % 6) + 1);
}

export default function Chat() {
  const { t } = useTranslation();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [openTabIds, setOpenTabIds] = useState<string[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  // Read by replies that land after an `await`. Those callbacks closed
  // over whatever tab was active when the send *started*, so reading
  // `activeSessionId` there answers the wrong question: switch tabs
  // mid-send and the reply would decide it had been seen, and the tab
  // it landed in would never show its unread marker.
  const activeSessionIdRef = useRef<string | null>(null);
  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);
  const [tabs, setTabs] = useState<Record<string, TabState>>({});
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const pageRef = useRef<HTMLDivElement>(null);
  const isActiveScreen = useIsActiveScreen(pageRef);

  // Skills: the installed catalog (global) + per-tab grants (in TabState)
  // + a small "run one now" form scoped to whichever tab is active.
  const [availableSkills, setAvailableSkills] = useState<SkillManifest[]>([]);
  const [skillToGrant, setSkillToGrant] = useState("");
  const [newMemoryDraft, setNewMemoryDraft] = useState("");
  const [runSkillName, setRunSkillName] = useState("");
  const [runSkillPayload, setRunSkillPayload] = useState("{}");
  const [runningSkill, setRunningSkill] = useState(false);
  const [importingSkill, setImportingSkill] = useState(false);

  // MCP servers: same catalog + per-tab-grants + "run one now" pattern as
  // Skills, plus one extra step Skills doesn't need — a server's tool
  // list isn't known until it's actually connected to, so running a
  // tool means picking a granted server first, then a live-fetched tool
  // from that specific server (mcpToolsForRun), not a single flat list.
  const [availableMcpServers, setAvailableMcpServers] = useState<McpServer[]>([]);
  const [mcpServerToGrant, setMcpServerToGrant] = useState("");
  const [runMcpServerId, setRunMcpServerId] = useState("");
  const [mcpToolsForRun, setMcpToolsForRun] = useState<McpTool[]>([]);
  const [loadingMcpTools, setLoadingMcpTools] = useState(false);
  const [runMcpToolName, setRunMcpToolName] = useState("");
  const [runMcpToolPayload, setRunMcpToolPayload] = useState("{}");
  const [runningMcpTool, setRunningMcpTool] = useState(false);

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
  // True once the user has manually picked a provider in this form session —
  // once set, selecting a role template stops overwriting it, since the
  // user's explicit choice should win over the template's suggestion.
  // Cross-provider fallback chain, staged locally until the agent is
  // actually created (add_agent_fallback_provider needs a real agentId) —
  // e.g. Anthropic fails, fall through to OpenRouter. Tried in this order,
  // only after the primary provider's own key rotation is exhausted.

  // New-session form state.
  const [newSessionAgentId, setNewSessionAgentId] = useState("");
  const [newSessionTitle, setNewSessionTitle] = useState("");

  // New-group-session form state.
  const [showNewGroup, setShowNewGroup] = useState(false);
  const [showNewSession, setShowNewSession] = useState(false);
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
  const sendingCount = Object.values(tabs).filter((tab) => tab.sending).length;


  useEffect(() => {
    refreshAgents().catch((e) => setError(String(e)));
    refreshSessions().catch((e) => setError(String(e)));
    invoke<SkillManifest[]>("list_skills")
      .then(setAvailableSkills)
      .catch((e) => setError(String(e)));
    invoke<McpServer[]>("list_mcp_servers")
      .then(setAvailableMcpServers)
      .catch((e) => setError(String(e)));
    invoke<MlCapabilityManifest[]>("list_ml_capabilities")
      .then(setAvailableMlCapabilities)
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [activeTab?.messages]);

  /* Agents are created on the Models screen now, and every screen stays
     mounted, so this one would otherwise keep showing the list it fetched
     when the app started. Refetch each time it becomes the visible screen
     — cheap, and the alternative is a session picker that does not list an
     agent the user just made. */
  useEffect(() => {
    if (!isActiveScreen) return;
    refreshAgents().catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActiveScreen]);

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
        const [fileGrants, skillGrants, mcpGrants, mlGrants, memories] = agent
          ? await Promise.all([
              invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: agent.id }),
              invoke<SkillAccessGrant[]>("list_skill_access_grants", { agentId: agent.id }),
              invoke<McpAccessGrant[]>("list_mcp_access_grants", { agentId: agent.id }),
              invoke<MlAccessGrant[]>("list_ml_access_grants_for_agent", { agentId: agent.id }),
              invoke<AgentMemory[]>("list_agent_memories", { agentId: agent.id }),
            ])
          : [[], [], [], [], []];
        patchTab(sessionId, { kind, messages, agent, members: [], fileGrants, skillGrants, mcpGrants, mlGrants, memories });
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

  /** Permanently deletes a session/group chat — there's no undo, so this
   *  confirms with the user first. If it's currently open, close its tab
   *  the same way the "×" button would, then refresh the sidebar list so
   *  the deleted entry actually disappears. */
  async function handleDeleteSession(sessionId: string, title: string) {
    const confirmed = await ask(t("chat.deleteSessionConfirm", { title }), {
      title: t("chat.deleteSessionConfirmTitle"),
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await invoke("delete_session", { sessionId });
      closeTab(sessionId);
      await refreshSessions();
    } catch (err) {
      setError(String(err));
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

  async function handleAddMemory(sessionId: string, content: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent || !content.trim()) return;
    setError(null);
    try {
      await invoke("add_agent_memory", { agentId: agent.id, content: content.trim() });
      const memories = await invoke<AgentMemory[]>("list_agent_memories", { agentId: agent.id });
      patchTab(sessionId, { memories });
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDeleteMemory(sessionId: string, id: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent) return;
    setError(null);
    try {
      await invoke("delete_agent_memory", { id });
      const memories = await invoke<AgentMemory[]>("list_agent_memories", { agentId: agent.id });
      patchTab(sessionId, { memories });
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

  async function handleGrantMcp(sessionId: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent || !mcpServerToGrant) return;
    setError(null);
    try {
      await invoke("grant_mcp_access", { agentId: agent.id, mcpServerId: mcpServerToGrant });
      const mcpGrants = await invoke<McpAccessGrant[]>("list_mcp_access_grants", { agentId: agent.id });
      patchTab(sessionId, { mcpGrants });
      setMcpServerToGrant("");
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeMcp(sessionId: string, id: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent) return;
    setError(null);
    try {
      await invoke("revoke_mcp_access", { id });
      const mcpGrants = await invoke<McpAccessGrant[]>("list_mcp_access_grants", { agentId: agent.id });
      patchTab(sessionId, { mcpGrants });
    } catch (err) {
      setError(String(err));
    }
  }

  /// Fetches the tool list for whichever granted server the "run a
  /// tool" form's server dropdown just selected — a live connect/
  /// list_tools call (see mcp_manager module docs: no persistent
  /// connection is kept per server), not a cached lookup.
  async function handleSelectMcpServerForRun(mcpServerId: string) {
    setRunMcpServerId(mcpServerId);
    setRunMcpToolName("");
    setMcpToolsForRun([]);
    if (!mcpServerId) return;
    setLoadingMcpTools(true);
    try {
      setMcpToolsForRun(await invoke<McpTool[]>("list_mcp_server_tools", { mcpServerId }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingMcpTools(false);
    }
  }

  /// Runs `runMcpToolName` on `runMcpServerId` with the JSON typed into
  /// `runMcpToolPayload`, and drops the result into the transcript the
  /// same way `handleRunSkill` does. Goes through the same Guardrails
  /// injection screen + per-agent allowlist as every other MCP tool call.
  async function handleRunMcpTool(sessionId: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent || !runMcpServerId || !runMcpToolName) return;
    setError(null);
    let payload: unknown;
    try {
      payload = JSON.parse(runMcpToolPayload || "{}");
    } catch {
      setError(t("chat.skillPayloadMustBeJson"));
      return;
    }
    setRunningMcpTool(true);
    try {
      await invoke("run_mcp_tool_in_session", {
        sessionId,
        agentId: agent.id,
        mcpServerId: runMcpServerId,
        toolName: runMcpToolName,
        arguments: payload,
      });
      const messages = await invoke<Message[]>("list_messages", { sessionId });
      patchTab(sessionId, { messages });
    } catch (err) {
      setError(String(err));
    } finally {
      setRunningMcpTool(false);
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
      } else if (tab.useFunctionCalling) {
        await invoke("send_chat_message_with_tools", { sessionId, content });
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
          hasUnseenReply: sessionId !== activeSessionIdRef.current,
        },
      }));
    } catch (err) {
      // A send the user stopped is not a failure, so it gets no error
      // banner — the backend returns exactly this string (see
      // `commands::CANCELLED`) precisely so the two can be told apart.
      //
      // The message list is reloaded rather than the draft being put
      // back. The user's own message was persisted before the provider
      // was ever called, so it is really in the transcript; restoring the
      // draft as well would leave the same text in two places and send it
      // twice if the user pressed Send again. Cancelling stops the reply,
      // not the message that asked for it.
      if (String(err) === CANCELLED) {
        const messages = await invoke<Message[]>("list_messages", { sessionId });
        patchTab(sessionId, { sending: false, messages });
      } else {
        setError(String(err));
        patchTab(sessionId, { sending: false });
      }
    }
  }

  function handleSend(e: FormEvent, sessionId: string) {
    e.preventDefault();
    void sendForSession(sessionId);
  }

  /// Asks the backend to stop the send in flight for this session.
  ///
  /// Deliberately does *not* clear `sending` itself. The request already
  /// on the wire cannot be aborted (see the Rust `cancel` module), so the
  /// command keeps running until it reaches its next cancellation
  /// boundary; `sending` is cleared by `sendForSession`'s own catch when
  /// it actually stops. Clearing it here would re-enable the composer
  /// while the old send was still running, and a second send could then
  /// start and take over the session's Stop button.
  async function handleStop(sessionId: string) {
    try {
      await invoke("cancel_send", { sessionId });
    } catch (err) {
      setError(String(err));
    }
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
      patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionIdRef.current });
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
        patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionIdRef.current });
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
      patchTab(sessionId, { messages, sending: false, hasUnseenReply: sessionId !== activeSessionIdRef.current });
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
        <span className="label">{t("chat.semanticSearchLabel")}</span>
        <div className="roster">
          {tab.mlGrants.length === 0 && <p className="field-hint">{t("chat.notGranted")}</p>}
          {tab.mlGrants.map((g) => (
            <div className="roster-item" key={g.id}>
              <span className="roster-name">{g.capabilityName}</span>
              <button
                className="tree-action"
                type="button"
                aria-label={t("chat.revokeAccessToCapability", { capabilityName: g.capabilityName })}
                onClick={() => handleRevokeMlCapability(sessionId, g.id)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        <div className="inspector-form">
          <select
            className="select"
            value={mlCapabilityToGrant}
            onChange={(e) => setMlCapabilityToGrant(e.target.value)}
          >
            <option value="">{t("chat.grantMlCapability")}</option>
            {availableMlCapabilities
              .filter((c) => !tab.mlGrants.some((g) => g.capabilityName === c.name))
              .map((c) => (
                <option key={c.name} value={c.name}>
                  {c.name}
                </option>
              ))}
          </select>
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            disabled={!mlCapabilityToGrant}
            onClick={() => handleGrantMlCapability(sessionId)}
          >
            {t("chat.grant")}
          </button>
        </div>
        {tab.mlGrants.some((g) => g.capabilityName === "semantic_search") && (
          <>
            <div className="inspector-form">
              <input
                className="input"
                type="search"
                placeholder={t("chat.searchFilesPlaceholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                disabled={!searchQuery.trim() || searching}
                onClick={() => handleSemanticSearch(sessionId)}
              >
                {searching ? t("chat.searching") : t("chat.search")}
              </button>
            </div>
            <button
              className="btn btn-ghost btn-sm inspector-action"
              type="button"
              disabled={indexing}
              onClick={() => handleBuildIndex(sessionId)}
            >
              {indexing ? t("chat.indexing") : t("chat.rebuildIndex")}
            </button>
          </>
        )}
        {tab.searchResults && (
          <div className="inspector-hits">
            {tab.searchResults.length === 0 && <p className="field-hint">{t("chat.noResults")}</p>}
            {tab.searchResults.map((r) => (
              <div className="hit" key={r.path}>
                <div className="hit-head">
                  <span className="hit-path">{r.path}</span>
                  <span className="hit-score">{r.score.toFixed(2)}</span>
                </div>
                <p className="hit-snippet">{r.excerpt}</p>
              </div>
            ))}
          </div>
        )}
      </>
    );
  }

  return (
    <>
      {/* The session tree moves to the shell's context sidebar, which is
          where the v2 layout puts "context for the active rail item". The
          markup below is unchanged and is still a child of this component
          in the React tree — only its DOM position moves — so every
          handler and every piece of state it closes over keeps working.

          Gated on being the visible screen because all eight screens stay
          mounted at once; without that, every screen with a sidebar would
          portal into the same host simultaneously. */}
      {isActiveScreen && (
      <ShellSidebar v2>
      <div className="sidebar-header">
        <span className="label">{t("chat.sessions")}</span>
        <button
          className="titlebar-btn"
          type="button"
          aria-label={t("chat.newSession")}
          aria-pressed={showNewSession}
          onClick={() => setShowNewSession((v) => !v)}
        >
          <Icon name="plus" size="sm" />
        </button>
      </div>

      <div className="sidebar-body">
        {showNewSession && (
          <form className="agent-form" onSubmit={handleCreateSession}>
            <select className="select" value={newSessionAgentId} onChange={(e) => setNewSessionAgentId(e.target.value)}>
              {agents.length === 0 && <option value="">{t("chat.noAgentsYetOption")}</option>}
              {agents.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name} ({a.providerName}/{a.model})
                </option>
              ))}
            </select>
            <input
              className="input"
              type="text"
              placeholder={t("chat.sessionTitlePlaceholder")}
              value={newSessionTitle}
              onChange={(e) => setNewSessionTitle(e.target.value)}
            />
            <button className="btn btn-primary btn-sm" type="submit">
              {t("chat.start")}
            </button>
          </form>
        )}

        <div className="label sidebar-section-label">{t("chat.independentSessions")}</div>
        {independentSessions.map((s) => (
          <div className="agent-row" key={s.id}>
            <button
              className="row agent-row-main"
              type="button"
              aria-current={openTabIds.includes(s.id) ? "true" : undefined}
              onClick={() => openTab(s.id, s.kind)}
            >
              <span className="name">{s.title}</span>
              {/* The dot is the only signal that a background tab has
                  answered, so it carries a label rather than relying on
                  colour and position alone. */}
              {tabs[s.id]?.hasUnseenReply && (
                <span className="status-dot" data-state="running" role="img" aria-label={t("chat.unseenReply")} />
              )}
            </button>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.deleteSession", { title: s.title })}
              onClick={() => void handleDeleteSession(s.id, s.title)}
            >
              <Icon name="trash" size="sm" />
            </button>
          </div>
        ))}
        {independentSessions.length === 0 && <p className="tree-empty">{t("chat.noSessions")}</p>}

        <div className="sidebar-group-head">
          <span className="label">{t("chat.groupChats")}</span>
          <button
            className="tree-action sidebar-group-add"
            type="button"
            aria-label={t("chat.newGroupChat")}
            aria-pressed={showNewGroup}
            onClick={() => setShowNewGroup((v) => !v)}
          >
            <Icon name="plus" size="sm" />
          </button>
        </div>

        {showNewGroup && (
          <form className="agent-form" onSubmit={handleCreateGroupSession}>
            <input
              className="input"
              type="text"
              placeholder={t("chat.meetingTitlePlaceholder")}
              value={newGroupTitle}
              onChange={(e) => setNewGroupTitle(e.target.value)}
            />
            <div className="group-member-picker">
              {agents.length === 0 && <span className="field-hint">{t("chat.noAgentsYet")}</span>}
              {agents.map((a) => (
                <label key={a.id} className="group-member">
                  <input
                    type="checkbox"
                    checked={newGroupAgentIds.includes(a.id)}
                    onChange={() => toggleNewGroupAgent(a.id)}
                  />
                  {a.name}
                </label>
              ))}
            </div>
            <button className="btn btn-primary btn-sm" type="submit">
              {t("chat.startMeeting")}
            </button>
          </form>
        )}

        {groupSessions.map((s) => (
          <div className="agent-row" key={s.id}>
            <button
              className="row agent-row-main"
              type="button"
              aria-current={openTabIds.includes(s.id) ? "true" : undefined}
              onClick={() => openTab(s.id, s.kind)}
            >
              <span className="name">{s.title}</span>
              {tabs[s.id]?.hasUnseenReply && (
                <span className="status-dot" data-state="running" role="img" aria-label={t("chat.unseenReply")} />
              )}
            </button>
            <button
              className="tree-action"
              type="button"
              aria-label={t("chat.deleteSession", { title: s.title })}
              onClick={() => void handleDeleteSession(s.id, s.title)}
            >
              <Icon name="trash" size="sm" />
            </button>
          </div>
        ))}
        {groupSessions.length === 0 && <p className="tree-empty">{t("chat.noGroupChats")}</p>}
      </div>

      <div className="sidebar-footer">
        <div className="row">
          {/* "Running" counts tabs with a request in flight, which is a
              fact this component holds. The design also shows a filter
              field; there is no session search to wire one to, so it is
              not drawn. */}
          <span className="name">
            {t("chat.sessionCount", { count: sessions.length })}
            {sendingCount > 0 && ` · ${t("chat.runningCount", { count: sendingCount })}`}
          </span>
        </div>
      </div>
      </ShellSidebar>
      )}

      {error &&
        (() => {
          const { code, rest } = parseErrorCode(error);
          return (
            <div className="callout chat-error-callout" data-kind="danger" role="alert">
              <Icon name="alert" />
              <div className="callout-body">
                {/* The code stays a separate chip: Design Principles has
                    every error carry one, and it is the part worth
                    copying into a bug report. */}
                {code && <span className="badge" data-kind="failed">{code}</span>} {rest}
                <div className="inspector-form">
                  {code && (
                    <button
                      className="btn btn-ghost btn-sm"
                      type="button"
                      onClick={() => void navigator.clipboard.writeText(error)}
                    >
                      {t("chat.copyErrorDetails")}
                    </button>
                  )}
                  <button
                    className="btn btn-ghost btn-sm"
                    type="button"
                    onClick={() => setError(null)}
                  >
                    {t("chat.dismissError")}
                  </button>
                </div>
              </div>
            </div>
          );
        })()}

      {/* Open tabs. The design has no tab strip — its chat screen shows one
          session at a time — but running several at once is the point of
          this app, so the strip stays and is drawn in the v2 vocabulary.

          Rendered unconditionally because it is what useIsActiveScreen
          anchors on: the hook needs one element that is always in the
          tree to find the pane from, and every other part of this screen
          comes and goes with the session. */}
      <div className="chat-tabs" ref={pageRef}>
        {openTabIds.map((id) => {
          const title = sessions.find((s) => s.id === id)?.title ?? "…";
          const tab = tabs[id];
          return (
            <div className={id === activeSessionId ? "chat-tab is-active" : "chat-tab"} key={id}>
              <button className="chat-tab-select" type="button" onClick={() => setActiveSessionId(id)}>
                {title}
                {tab?.sending && (
                  <span className="status-dot" data-state="running" role="img" aria-label={t("chat.sending")} />
                )}
                {tab?.hasUnseenReply && (
                  <span className="status-dot" data-state="ok" role="img" aria-label={t("chat.unseenReply")} />
                )}
              </button>
              <button
                className="chat-tab-close"
                type="button"
                aria-label={t("chat.closeTab", { title })}
                onClick={() => closeTab(id)}
              >
                ×
              </button>
            </div>
          );
        })}
      </div>

        {/* The workspace header the design gives every screen. Chat is the last
          to get one: the tab strip was doing part of the job, but a strip of
          tabs cannot say what the tab in front is, nor hold the actions that
          belong to it. */}
      {activeSessionId && activeTab && (
        <div className="workspace-header">
          {activeTab.kind === "group" ? (
            <Icon name="chat" size="sm" />
          ) : (
            activeTab.agent && (
              <span className="avatar" data-agent="1" aria-hidden="true">
                {initials(activeTab.agent.name)}
              </span>
            )
          )}
          <span className="workspace-title">{sessions.find((s) => s.id === activeSessionId)?.title ?? ""}</span>
          {activeTab.sending && (
            <span className="badge" data-kind="running">
              {t("chat.sending")}
            </span>
          )}
          {activeTab.kind === "independent" && activeTab.agent && (
            <span className="badge" data-kind={activeTab.agent.providerKind === "local" ? "local" : "cloud"}>
              {activeTab.agent.providerName}
            </span>
          )}
          {activeTab.kind === "group" && (
            <div className="workspace-actions">
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                disabled={activeTab.sending}
                onClick={() => handleEndMeeting(activeSessionId)}
              >
                {t("chat.endMeeting")}
              </button>
            </div>
          )}
        </div>
      )}

      {!activeSessionId && <div className="workspace-body"><div className="pane"><p className="pane-intro">{t("chat.pickOrStartSession")}</p></div></div>}

        {activeSessionId && activeTab && (
          <>
            {activeTab.agent && (
              <ShellInspector v2>
                <div className="inspector-header">
                  <span className="label">{t("chat.sessionDetails")}</span>
                </div>
                <div className="inspector-body">
                  {/* No "Agent" header above this block: the panel is titled
                      Session and this is self-evidently the agent. The design
                      makes the same point — a header with nothing to
                      distinguish it from the row below is a line of noise. */}
                  <section className="inspector-section">
                    <div className="inspector-identity">
                      <span className="avatar avatar-lg" data-agent="1" aria-hidden="true">
                        {initials(activeTab.agent.name)}
                      </span>
                      <div className="inspector-identity-text">
                        <div className="inspector-identity-name">{activeTab.agent.name}</div>
                        {activeTab.agent.roleTemplate && (
                          <div className="inspector-identity-sub">{activeTab.agent.roleTemplate}</div>
                        )}
                      </div>
                    </div>
                    <dl className="kv">
                      <dt>{t("chat.provider")}</dt>
                      <dd>
                        <span className="badge" data-kind={activeTab.agent.providerKind === "local" ? "local" : "cloud"}>
                          {activeTab.agent.providerName}
                        </span>
                      </dd>
                      <dt>{t("chat.model")}</dt>
                      <dd className="mono">{activeTab.agent.model}</dd>
                    </dl>
                  </section>

                  <section className="inspector-section">
                    <span className="label label-lead">{t("chat.files")}</span>
                    <div className="roster">
                      {activeTab.fileGrants.length === 0 && (
                        <p className="field-hint">{t("chat.noFoldersGranted")}</p>
                      )}
                      {activeTab.fileGrants.map((g) => (
                        <div className="roster-item" key={g.id}>
                          <Icon name="folder" size="sm" />
                          <span className="roster-name mono">{g.folderPath}</span>
                          <button
                            className="tree-action"
                            type="button"
                            aria-label={t("chat.revokeAccessToFolder", { folderPath: g.folderPath })}
                            onClick={() => handleRevokeGrant(activeSessionId, g.id)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <button
                      className="btn btn-secondary btn-sm inspector-action"
                      type="button"
                      onClick={() => handleGrantFolder(activeSessionId)}
                    >
                      {t("chat.grantFolder")}
                    </button>
                  </section>

                  <section className="inspector-section">
                    <span className="label">{t("chat.skills")}</span>
                    <div className="roster">
                      {activeTab.skillGrants.length === 0 && <p className="field-hint">{t("chat.noneGranted")}</p>}
                      {activeTab.skillGrants.map((g) => (
                        <div className="roster-item" key={g.id}>
                          <Icon name="skills" size="sm" />
                          <span className="roster-name">{g.skillName}</span>
                          <button
                            className="tree-action"
                            type="button"
                            aria-label={t("chat.revokeAccessToSkill", { skillName: g.skillName })}
                            onClick={() => handleRevokeSkill(activeSessionId, g.id)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <div className="inspector-form">
                      <select className="select" value={skillToGrant} onChange={(e) => setSkillToGrant(e.target.value)}>
                        <option value="">{t("chat.grantASkill")}</option>
                        {availableSkills
                          .filter((s) => !activeTab.skillGrants.some((g) => g.skillName === s.name))
                          .map((s) => (
                            <option key={s.name} value={s.name}>
                              {s.name} {s.source === "custom" ? t("chat.custom") : ""}
                            </option>
                          ))}
                      </select>
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        disabled={!skillToGrant}
                        onClick={() => handleGrantSkill(activeSessionId)}
                      >
                        {t("chat.grant")}
                      </button>
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={importingSkill}
                        onClick={() => handleImportSkill()}
                      >
                        {t("chat.importSkill")}
                      </button>
                    </div>
                    <p className="field-hint">{t("chat.skillImportWarning")}</p>
                    {functionCallingEligible(activeTab) && (
                      <label className="inspector-toggle">
                        <input
                          type="checkbox"
                          checked={activeTab.useFunctionCalling}
                          onChange={(e) => patchTab(activeSessionId, { useFunctionCalling: e.target.checked })}
                        />
                        {t("chat.letAgentCallSkills")}
                      </label>
                    )}
                    {activeTab.skillGrants.length > 0 && (
                      <div className="inspector-form">
                        <select className="select" value={runSkillName} onChange={(e) => setRunSkillName(e.target.value)}>
                          <option value="">{t("chat.runASkill")}</option>
                          {activeTab.skillGrants.map((g) => (
                            <option key={g.id} value={g.skillName}>
                              {g.skillName}
                            </option>
                          ))}
                        </select>
                        <input
                          className="input input-mono"
                          type="text"
                          placeholder={t("chat.jsonPayloadPlaceholder")}
                          value={runSkillPayload}
                          onChange={(e) => setRunSkillPayload(e.target.value)}
                        />
                        <button
                          className="btn btn-ghost btn-sm"
                          type="button"
                          disabled={!runSkillName || runningSkill}
                          onClick={() => handleRunSkill(activeSessionId)}
                        >
                          {runningSkill ? t("chat.running") : t("chat.run")}
                        </button>
                      </div>
                    )}
                  </section>

                  <section className="inspector-section">
                    <span className="label">{t("chat.memories")}</span>
                    <div className="roster">
                      {activeTab.memories.length === 0 && <p className="field-hint">{t("chat.noMemoriesYet")}</p>}
                      {activeTab.memories.map((m) => (
                        <div className="roster-item" key={m.id}>
                          <span className="roster-name">{m.content}</span>
                          <button
                            className="tree-action"
                            type="button"
                            aria-label={t("chat.forgetMemory", { content: m.content })}
                            onClick={() => handleDeleteMemory(activeSessionId, m.id)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <div className="inspector-form">
                      <input
                        className="input"
                        type="text"
                        placeholder={t("chat.rememberSomethingPlaceholder")}
                        value={newMemoryDraft}
                        onChange={(e) => setNewMemoryDraft(e.target.value)}
                      />
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        disabled={!newMemoryDraft.trim()}
                        onClick={() => {
                          void handleAddMemory(activeSessionId, newMemoryDraft);
                          setNewMemoryDraft("");
                        }}
                      >
                        {t("chat.remember")}
                      </button>
                    </div>
                  </section>

                  <section className="inspector-section">
                    <span className="label">{t("chat.mcpServers")}</span>
                    <div className="roster">
                      {activeTab.mcpGrants.length === 0 && <p className="field-hint">{t("chat.noneGranted")}</p>}
                      {activeTab.mcpGrants.map((g) => {
                        const server = availableMcpServers.find((s) => s.id === g.mcpServerId);
                        return (
                          <div className="roster-item" key={g.id}>
                            <span className="roster-name">{server?.name ?? g.mcpServerId}</span>
                            <button
                              className="tree-action"
                              type="button"
                              aria-label={t("chat.revokeAccessToMcpServer", {
                                serverName: server?.name ?? g.mcpServerId,
                              })}
                              onClick={() => handleRevokeMcp(activeSessionId, g.id)}
                            >
                              ×
                            </button>
                          </div>
                        );
                      })}
                    </div>
                    <div className="inspector-form">
                      <select
                        className="select"
                        value={mcpServerToGrant}
                        onChange={(e) => setMcpServerToGrant(e.target.value)}
                      >
                        <option value="">{t("chat.grantAnMcpServer")}</option>
                        {availableMcpServers
                          .filter((s) => !activeTab.mcpGrants.some((g) => g.mcpServerId === s.id))
                          .map((s) => (
                            <option key={s.id} value={s.id}>
                              {s.name}
                            </option>
                          ))}
                      </select>
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        disabled={!mcpServerToGrant}
                        onClick={() => handleGrantMcp(activeSessionId)}
                      >
                        {t("chat.grant")}
                      </button>
                    </div>
                    {activeTab.mcpGrants.length > 0 && (
                      <div className="inspector-form">
                        <select
                          className="select"
                          value={runMcpServerId}
                          onChange={(e) => handleSelectMcpServerForRun(e.target.value)}
                        >
                          <option value="">{t("chat.runAnMcpTool")}</option>
                          {activeTab.mcpGrants.map((g) => {
                            const server = availableMcpServers.find((s) => s.id === g.mcpServerId);
                            return (
                              <option key={g.id} value={g.mcpServerId}>
                                {server?.name ?? g.mcpServerId}
                              </option>
                            );
                          })}
                        </select>
                        <select
                          className="select"
                          value={runMcpToolName}
                          onChange={(e) => setRunMcpToolName(e.target.value)}
                          disabled={!runMcpServerId || loadingMcpTools}
                        >
                          <option value="">{loadingMcpTools ? t("chat.loadingTools") : t("chat.selectTool")}</option>
                          {mcpToolsForRun.map((tool) => (
                            <option key={tool.name} value={tool.name}>
                              {tool.name}
                            </option>
                          ))}
                        </select>
                        <input
                          className="input input-mono"
                          type="text"
                          placeholder={t("chat.jsonPayloadPlaceholder")}
                          value={runMcpToolPayload}
                          onChange={(e) => setRunMcpToolPayload(e.target.value)}
                        />
                        <button
                          className="btn btn-ghost btn-sm"
                          type="button"
                          disabled={!runMcpServerId || !runMcpToolName || runningMcpTool}
                          onClick={() => handleRunMcpTool(activeSessionId)}
                        >
                          {runningMcpTool ? t("chat.running") : t("chat.run")}
                        </button>
                      </div>
                    )}
                  </section>

                  <section className="inspector-section">
                    {renderSemanticSearchSection(activeSessionId, activeTab)}
                  </section>
                </div>
              </ShellInspector>
            )}
            {activeTab.kind === "group" && (
              <ShellInspector v2>
                <div className="inspector-header">
                  <span className="label">{t("chat.meeting")}</span>
                </div>
                <div className="inspector-body">
                  <section className="inspector-section">
                    <span className="label label-lead">{t("chat.participants")}</span>
                    <div className="roster">
                      {activeTab.members.length === 0 && <p className="field-hint">{t("chat.noMembers")}</p>}
                      {activeTab.members.map((m, i) => (
                        <div className="roster-item" key={m.id}>
                          <span className="avatar" data-agent={String((i % 6) + 1)} aria-hidden="true">
                            {initials(m.name)}
                          </span>
                          <span className="roster-name">
                            {m.name}
                            <span className="roster-sub mono">{m.model}</span>
                          </span>
                          <span className="badge" data-kind={m.providerKind === "local" ? "local" : "cloud"}>
                            {m.providerName}
                          </span>
                        </div>
                      ))}
                    </div>
                  </section>

                  {/* The design shows a turn-limit meter here. The loop
                      guard that stops a meeting running away is real, but
                      its cap and the turn already reached are not exposed
                      to the UI, so there is no number to fill a meter
                      with — only the count this session will advance by
                      when asked, which is a control rather than a state. */}
                  <section className="inspector-section">
                    <span className="label">{t("chat.turns")}</span>
                    <div className="inspector-form">
                      <input
                        className="input input-num"
                        type="number"
                        min={1}
                        max={6}
                        value={autoContinueTurns}
                        disabled={activeTab.sending}
                        aria-label={t("chat.autoContinueTitle")}
                        onChange={(e) => setAutoContinueTurns(Math.max(1, Math.min(6, Number(e.target.value) || 1)))}
                      />
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        disabled={activeTab.sending}
                        onClick={() => handleAutoContinue(activeSessionId, autoContinueTurns)}
                      >
                        {t("chat.continueTurns", { turns: autoContinueTurns })}
                      </button>
                    </div>
                    <button
                      className="btn btn-ghost btn-sm inspector-action"
                      type="button"
                      disabled={activeTab.sending}
                      onClick={() => handleAdvanceTurn(activeSessionId)}
                    >
                      {t("chat.letThemContinue")}
                    </button>
                  </section>

                  <section className="inspector-section">
                    <span className="label label-lead">{t("chat.filesSharedWithMeeting")}</span>
                    <div className="roster">
                      {activeTab.fileGrants.length === 0 && <p className="field-hint">{t("chat.noFoldersGranted")}</p>}
                      {activeTab.fileGrants.map((g) => (
                        <div className="roster-item" key={g.id}>
                          <Icon name="folder" size="sm" />
                          <span className="roster-name mono">{g.folderPath}</span>
                          <button
                            className="tree-action"
                            type="button"
                            aria-label={t("chat.revokeAccessToFolder", { folderPath: g.folderPath })}
                            onClick={() => handleRevokeGrant(activeSessionId, g.id)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <button
                      className="btn btn-secondary btn-sm inspector-action"
                      type="button"
                      disabled={activeTab.members.length === 0}
                      onClick={() => handleGrantFolder(activeSessionId)}
                    >
                      {t("chat.grantFolder")}
                    </button>
                  </section>

                  <section className="inspector-section">
                    {renderSemanticSearchSection(activeSessionId, activeTab)}
                  </section>
                </div>
              </ShellInspector>
            )}

            {/* The local→cloud boundary confirmation (E6004) stays in the
                conversation, not the inspector. It is a question about the
                message about to be sent, and it has to be answered before
                anything else happens — a panel the user may have collapsed
                is the wrong place for a blocking prompt. */}
            {activeTab.pendingBoundary && (
              <div className="callout boundary-confirm" data-kind="warning">
                <Icon name="alert" />
                <div className="callout-body">
                  <strong>{t("chat.boundaryConfirmTitle")}</strong>
                  {t("chat.boundaryConfirmBody")}
                  <pre className="boundary-preview">{activeTab.pendingBoundary}</pre>
                  <div className="inspector-form">
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      onClick={() => handleConfirmBoundary(activeSessionId)}
                    >
                      {t("chat.sendToCloud")}
                    </button>
                    <button
                      className="btn btn-ghost btn-sm"
                      type="button"
                      onClick={() => handleCancelBoundary(activeSessionId)}
                    >
                      {t("chat.cancel")}
                    </button>
                  </div>
                </div>
              </div>
            )}

            <div className="stream">
              {activeTab.messages.length === 0 && <p className="field-hint">{t("chat.noMessagesYet")}</p>}
              {activeTab.messages.map((m, i) => {
                const speaker =
                  activeTab.kind === "group" && m.agentId
                    ? (activeTab.members.find((mem) => mem.id === m.agentId)?.name ?? m.role)
                    : m.role === "user"
                      ? t("chat.you")
                      : (activeTab.agent?.name ?? m.role);
                const prev = activeTab.messages[i - 1];
                /* Consecutive turns from one speaker drop their header and
                   avatar, which is what the design's data-same does. Keyed
                   on agentId as well as role so two agents answering in a
                   row in a group chat are not collapsed into one. */
                const same = prev?.role === m.role && prev?.agentId === m.agentId;
                return (
                  <article className="msg" data-from={m.role === "user" ? "user" : "agent"} data-same={same} key={m.id}>
                    <span className="avatar" data-agent={avatarSlot(m, activeTab)} aria-hidden="true">
                      {initials(speaker)}
                    </span>
                    <div className="msg-head">
                      <span className="msg-who">{speaker}</span>
                      {m.role !== "user" && activeTab.agent?.model && (
                        <span className="msg-meta">{activeTab.agent.model}</span>
                      )}
                    </div>
                    <div className="msg-body">{m.content}</div>
                  </article>
                );
              })}
              {activeTab.sending && (
                <article className="msg" data-from="agent">
                  <span className="avatar" data-agent="1" aria-hidden="true">
                    {initials(activeTab.agent?.name ?? "AI")}
                  </span>
                  <div className="msg-head">
                    <span className="msg-who">{activeTab.agent?.name ?? t("chat.assistant")}</span>
                  </div>
                  <div className="msg-body">
                    <span className="thinking">
                      {t("chat.sending")}
                      <span className="dots" aria-hidden="true">
                        <i />
                        <i />
                        <i />
                      </span>
                    </span>
                  </div>
                </article>
              )}
              <div ref={bottomRef} />
            </div>

            <div className="composer">
              <form
                className="composer-box"
                onSubmit={(e) => handleSend(e, activeSessionId)}
              >
                <label className="sr-only" htmlFor={`composer-${activeSessionId}`}>
                  {activeTab.kind === "group"
                    ? t("chat.messagePlaceholderGroup")
                    : t("chat.messagePlaceholderIndependent")}
                </label>
                {/* A textarea, not a single-line input: a prompt is often
                    several lines, and the design sizes it for that. Enter
                    sends and Shift+Enter breaks the line, which is the
                    convention the composer hint states. */}
                <textarea
                  id={`composer-${activeSessionId}`}
                  className="composer-input"
                  rows={3}
                  placeholder={
                    activeTab.kind === "group"
                      ? t("chat.messagePlaceholderGroup")
                      : t("chat.messagePlaceholderIndependent")
                  }
                  value={activeTab.draft}
                  onChange={(e) => patchTab(activeSessionId, { draft: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      if (activeTab.draft.trim() && !activeTab.sending) {
                        void handleSend(e, activeSessionId);
                      }
                    }
                  }}
                />
                <div className="composer-bar">
                  <span className="spacer" />
                  {activeTab.agent && (
                    <span className="composer-model">
                      <Icon name={activeTab.agent.providerKind === "local" ? "local" : "cloud"} size="sm" />
                      {activeTab.agent.model}
                    </span>
                  )}
                  <span className="composer-hint">
                    <kbd>{t("chat.enterKey")}</kbd>
                  </span>
                  {/* The design pairs Send with a Stop in the same slot.
                      Stop is only drawn where it can actually do
                      something: an independent session's send goes
                      through `cancel_send`, a group turn has no
                      equivalent yet, so there the button keeps reporting
                      that it is sending rather than offering a control
                      that would do nothing. */}
                  {activeTab.sending && activeTab.kind !== "group" ? (
                    <button
                      className="btn btn-secondary btn-sm composer-send"
                      type="button"
                      onClick={() => void handleStop(activeSessionId)}
                    >
                      {t("chat.stop")}
                    </button>
                  ) : (
                    <button
                      className="btn btn-primary btn-sm composer-send"
                      type="submit"
                      disabled={!activeTab.draft.trim() || activeTab.sending}
                    >
                      {activeTab.sending ? t("chat.sending") : t("chat.send")}
                    </button>
                  )}
                </div>
              </form>
            </div>
          </>
        )}
    </>
  );
}
