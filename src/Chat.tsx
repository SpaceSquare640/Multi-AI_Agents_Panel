import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import type { Agent, CuratedModel, FileAccessGrant, Message, RoleTemplate, Session } from "./types";
import "./Chat.css";

const PROVIDER_OPTIONS = ["anthropic", "openrouter", "ollama"] as const;

/// Per-session state, kept independently for every *open* tab so that
/// sending a message in one session never blocks, resets, or loses state
/// in another — this is what makes "multiple agents in parallel" real
/// rather than just a session picker. See dev order step 10 /
/// Session Types.md.
interface TabState {
  messages: Message[];
  agent: Agent | null;
  fileGrants: FileAccessGrant[];
  draft: string;
  sending: boolean;
  /** True while this tab is open but not the one currently in view — used
   *  to show a "new reply" dot without disturbing the active tab. */
  hasUnseenReply: boolean;
}

function emptyTab(): TabState {
  return { messages: [], agent: null, fileGrants: [], draft: "", sending: false, hasUnseenReply: false };
}

export default function Chat() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [openTabIds, setOpenTabIds] = useState<string[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [tabs, setTabs] = useState<Record<string, TabState>>({});
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

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

  // Role templates ("1 人公司"): default (built-in) + custom (user-authored).
  const [roleTemplates, setRoleTemplates] = useState<RoleTemplate[]>([]);
  const [showNewTemplate, setShowNewTemplate] = useState(false);
  const [templateName, setTemplateName] = useState("");
  const [templateDescription, setTemplateDescription] = useState("");
  const [templatePrompt, setTemplatePrompt] = useState("");

  // New-session form state.
  const [newSessionAgentId, setNewSessionAgentId] = useState("");
  const [newSessionTitle, setNewSessionTitle] = useState("");

  async function refreshAgents() {
    const list = await invoke<Agent[]>("list_agents");
    setAgents(list);
    if (list.length > 0 && !newSessionAgentId) setNewSessionAgentId(list[0].id);
  }

  async function refreshSessions() {
    const all = await invoke<Session[]>("list_sessions");
    setSessions(all.filter((s) => s.kind === "independent"));
  }

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
    if (template.suggestedProviderName) setNewAgentProvider(template.suggestedProviderName);
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
        providerKind: newAgentProvider === "ollama" ? "local" : "cloud",
        providerName: newAgentProvider,
        model: newAgentModel,
      });
      setNewAgentName("");
      setNewAgentSystemPrompt("");
      setNewAgentTemplateId("");
      setShowNewAgent(false);
      await refreshAgents();
      setNewSessionAgentId(agent.id);
    } catch (err) {
      setError(String(err));
    }
  }

  /// Opens a session as a tab (loading its messages/agent/grants the
  /// first time) and brings it to the front. Already-open tabs keep
  /// whatever state they had — switching tabs never re-fetches or resets.
  async function openTab(sessionId: string) {
    setActiveSessionId(sessionId);
    patchTab(sessionId, { hasUnseenReply: false });
    setOpenTabIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));
    if (tabs[sessionId]) return; // already loaded

    try {
      const [messages, agentId] = await Promise.all([
        invoke<Message[]>("list_messages", { sessionId }),
        invoke<string | null>("get_session_agent_id", { sessionId }),
      ]);
      const agent = agents.find((a) => a.id === agentId) ?? null;
      const fileGrants = agent
        ? await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: agent.id })
        : [];
      patchTab(sessionId, { messages, agent, fileGrants });
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
      setError("Create an agent first.");
      return;
    }
    try {
      const agent = agents.find((a) => a.id === newSessionAgentId);
      const title = newSessionTitle || `Chat with ${agent?.name ?? "agent"}`;
      const session = await invoke<Session>("create_independent_session", {
        title,
        agentId: newSessionAgentId,
      });
      setNewSessionTitle("");
      await refreshSessions();
      await openTab(session.id);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleCreateTemplate(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await invoke("create_custom_role_template", {
        name: templateName,
        description: templateDescription,
        systemPrompt: templatePrompt,
        suggestedProviderKind: null,
        suggestedProviderName: null,
        suggestedModel: null,
      });
      setTemplateName("");
      setTemplateDescription("");
      setTemplatePrompt("");
      setShowNewTemplate(false);
      await refreshRoleTemplates();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleGrantFolder(sessionId: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent) return;
    setError(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return; // user cancelled the picker
      await invoke("grant_folder_access", { agentId: agent.id, folderPath: folder });
      const fileGrants = await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: agent.id });
      patchTab(sessionId, { fileGrants });
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeGrant(sessionId: string, id: string) {
    const agent = tabs[sessionId]?.agent;
    if (!agent) return;
    setError(null);
    try {
      await invoke("revoke_file_access_grant", { id });
      const fileGrants = await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId: agent.id });
      patchTab(sessionId, { fileGrants });
    } catch (err) {
      setError(String(err));
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
      await invoke("send_chat_message", { sessionId, content });
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

  return (
    <div className="chat-page">
      <aside className="chat-sidebar">
        <h2>Sessions</h2>
        <ul className="chat-session-list">
          {sessions.map((s) => (
            <li key={s.id}>
              <button
                className={openTabIds.includes(s.id) ? "active" : ""}
                onClick={() => openTab(s.id)}
              >
                {s.title}
                {tabs[s.id]?.hasUnseenReply && <span className="chat-unread-dot" />}
              </button>
            </li>
          ))}
          {sessions.length === 0 && <li className="chat-empty">No sessions yet.</li>}
        </ul>

        <h3>New session</h3>
        <form className="chat-form" onSubmit={handleCreateSession}>
          <select value={newSessionAgentId} onChange={(e) => setNewSessionAgentId(e.target.value)}>
            {agents.length === 0 && <option value="">No agents yet</option>}
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name} ({a.providerName}/{a.model})
              </option>
            ))}
          </select>
          <input
            type="text"
            placeholder="Session title (optional)"
            value={newSessionTitle}
            onChange={(e) => setNewSessionTitle(e.target.value)}
          />
          <button type="submit" disabled={agents.length === 0}>
            Start
          </button>
        </form>

        <button className="chat-link-button" onClick={() => setShowNewAgent((v) => !v)}>
          {showNewAgent ? "Cancel" : "+ New agent"}
        </button>
        {showNewAgent && (
          <form className="chat-form" onSubmit={handleCreateAgent}>
            <select value={newAgentTemplateId} onChange={(e) => handleSelectTemplate(e.target.value)}>
              <option value="">No role template (write your own prompt)</option>
              {roleTemplates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name} {t.source === "custom" ? "(custom)" : ""}
                </option>
              ))}
            </select>
            <input
              type="text"
              placeholder="Agent name"
              value={newAgentName}
              onChange={(e) => setNewAgentName(e.target.value)}
              required
            />
            <select value={newAgentProvider} onChange={(e) => setNewAgentProvider(e.target.value)}>
              {PROVIDER_OPTIONS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            <select value={newAgentModel} onChange={(e) => setNewAgentModel(e.target.value)}>
              {newAgentModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
            <textarea
              rows={3}
              placeholder="System prompt (optional — filled in automatically by a role template)"
              value={newAgentSystemPrompt}
              onChange={(e) => setNewAgentSystemPrompt(e.target.value)}
            />
            <button type="submit">Create agent</button>
          </form>
        )}

        <button className="chat-link-button" onClick={() => setShowNewTemplate((v) => !v)}>
          {showNewTemplate ? "Cancel" : "+ New role template"}
        </button>
        {showNewTemplate && (
          <form className="chat-form" onSubmit={handleCreateTemplate}>
            <input
              type="text"
              placeholder="Template name"
              value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder="Short description"
              value={templateDescription}
              onChange={(e) => setTemplateDescription(e.target.value)}
              required
            />
            <textarea
              rows={3}
              placeholder="System prompt"
              value={templatePrompt}
              onChange={(e) => setTemplatePrompt(e.target.value)}
              required
            />
            <button type="submit">Save template</button>
          </form>
        )}
      </aside>

      <main className="chat-main">
        {error && (
          <div className="chat-error" role="alert">
            {error}
            <button onClick={() => setError(null)}>×</button>
          </div>
        )}

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
                  <button className="chat-tab-close" onClick={() => closeTab(id)} title="Close tab">
                    ×
                  </button>
                </div>
              );
            })}
          </div>
        )}

        {!activeSessionId && (
          <p className="chat-empty">
            Pick or start a session on the left. You can open several at once — each keeps chatting
            independently, even in the background.
          </p>
        )}

        {activeSessionId && activeTab && (
          <>
            {activeTab.agent && (
              <div className="chat-header">
                <div>
                  Chatting with <strong>{activeTab.agent.name}</strong> ({activeTab.agent.providerName}/
                  {activeTab.agent.model})
                </div>
                <div className="chat-file-access">
                  <span>Files:</span>
                  {activeTab.fileGrants.length === 0 && (
                    <span className="chat-empty">no folders granted</span>
                  )}
                  {activeTab.fileGrants.map((g) => (
                    <span key={g.id} className="chat-file-chip">
                      {g.folderPath}
                      <button onClick={() => handleRevokeGrant(activeSessionId, g.id)} title="Revoke access">
                        ×
                      </button>
                    </span>
                  ))}
                  <button className="chat-link-button" onClick={() => handleGrantFolder(activeSessionId)}>
                    + Grant folder…
                  </button>
                </div>
              </div>
            )}
            <div className="chat-thread">
              {activeTab.messages.length === 0 && (
                <p className="chat-empty">No messages yet — say hello.</p>
              )}
              {activeTab.messages.map((m) => (
                <div key={m.id} className={`chat-bubble chat-bubble-${m.role}`}>
                  <div className="chat-bubble-role">{m.role}</div>
                  <div className="chat-bubble-content">{m.content}</div>
                </div>
              ))}
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
                placeholder="Type a message… (use @file:C:\path\to\file.txt to attach a granted file)"
                value={activeTab.draft}
                onChange={(e) => patchTab(activeSessionId, { draft: e.target.value })}
              />
              <button type="submit" disabled={!activeTab.draft.trim()}>
                {activeTab.sending ? "Sending…" : "Send"}
              </button>
            </form>
          </>
        )}
      </main>
    </div>
  );
}
