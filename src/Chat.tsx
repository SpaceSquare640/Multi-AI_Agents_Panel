import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import type { Agent, CuratedModel, FileAccessGrant, Message, Session } from "./types";
import "./Chat.css";

const PROVIDER_OPTIONS = ["anthropic", "openrouter", "ollama"] as const;

export default function Chat() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [activeAgent, setActiveAgent] = useState<Agent | null>(null);
  const [fileGrants, setFileGrants] = useState<FileAccessGrant[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // New-agent form state.
  const [showNewAgent, setShowNewAgent] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentProvider, setNewAgentProvider] = useState<string>("openrouter");
  const [newAgentModels, setNewAgentModels] = useState<CuratedModel[]>([]);
  const [newAgentModel, setNewAgentModel] = useState("");

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

  async function refreshMessages(sessionId: string) {
    setMessages(await invoke<Message[]>("list_messages", { sessionId }));
  }

  async function refreshFileGrants(agentId: string) {
    setFileGrants(await invoke<FileAccessGrant[]>("list_file_access_grants", { agentId }));
  }

  useEffect(() => {
    refreshAgents().catch((e) => setError(String(e)));
    refreshSessions().catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!activeSessionId) {
      setActiveAgent(null);
      return;
    }
    refreshMessages(activeSessionId).catch((e) => setError(String(e)));
    invoke<string | null>("get_session_agent_id", { sessionId: activeSessionId })
      .then((agentId) => setActiveAgent(agents.find((a) => a.id === agentId) ?? null))
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeSessionId, agents]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    if (activeAgent) {
      refreshFileGrants(activeAgent.id).catch((e) => setError(String(e)));
    } else {
      setFileGrants([]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeAgent]);

  useEffect(() => {
    if (!showNewAgent) return;
    invoke<CuratedModel[]>("list_curated_models", { provider: newAgentProvider })
      .then((models) => {
        setNewAgentModels(models);
        setNewAgentModel(models[0]?.id ?? "");
      })
      .catch((e) => setError(String(e)));
  }, [showNewAgent, newAgentProvider]);

  async function handleCreateAgent(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const agent = await invoke<Agent>("create_agent", {
        name: newAgentName,
        roleTemplate: null,
        providerKind: newAgentProvider === "ollama" ? "local" : "cloud",
        providerName: newAgentProvider,
        model: newAgentModel,
      });
      setNewAgentName("");
      setShowNewAgent(false);
      await refreshAgents();
      setNewSessionAgentId(agent.id);
    } catch (err) {
      setError(String(err));
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
      setActiveSessionId(session.id);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleGrantFolder() {
    if (!activeAgent) return;
    setError(null);
    try {
      const folder = await openFolderPicker({ directory: true, multiple: false });
      if (!folder) return; // user cancelled the picker
      await invoke("grant_folder_access", { agentId: activeAgent.id, folderPath: folder });
      await refreshFileGrants(activeAgent.id);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRevokeGrant(id: string) {
    if (!activeAgent) return;
    setError(null);
    try {
      await invoke("revoke_file_access_grant", { id });
      await refreshFileGrants(activeAgent.id);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleSend(e: FormEvent) {
    e.preventDefault();
    if (!activeSessionId || !draft.trim()) return;
    setError(null);
    setSending(true);
    const content = draft;
    setDraft("");
    try {
      await invoke("send_chat_message", { sessionId: activeSessionId, content });
      await refreshMessages(activeSessionId);
    } catch (err) {
      setError(String(err));
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="chat-page">
      <aside className="chat-sidebar">
        <h2>Sessions</h2>
        <ul className="chat-session-list">
          {sessions.map((s) => (
            <li key={s.id}>
              <button
                className={s.id === activeSessionId ? "active" : ""}
                onClick={() => setActiveSessionId(s.id)}
              >
                {s.title}
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
            <button type="submit">Create agent</button>
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

        {!activeSessionId && <p className="chat-empty">Pick or start a session on the left.</p>}

        {activeSessionId && (
          <>
            {activeAgent && (
              <div className="chat-header">
                <div>
                  Chatting with <strong>{activeAgent.name}</strong> ({activeAgent.providerName}/
                  {activeAgent.model})
                </div>
                <div className="chat-file-access">
                  <span>Files:</span>
                  {fileGrants.length === 0 && <span className="chat-empty">no folders granted</span>}
                  {fileGrants.map((g) => (
                    <span key={g.id} className="chat-file-chip">
                      {g.folderPath}
                      <button onClick={() => handleRevokeGrant(g.id)} title="Revoke access">
                        ×
                      </button>
                    </span>
                  ))}
                  <button className="chat-link-button" onClick={handleGrantFolder}>
                    + Grant folder…
                  </button>
                </div>
              </div>
            )}
            <div className="chat-thread">
              {messages.length === 0 && <p className="chat-empty">No messages yet — say hello.</p>}
              {messages.map((m) => (
                <div key={m.id} className={`chat-bubble chat-bubble-${m.role}`}>
                  <div className="chat-bubble-role">{m.role}</div>
                  <div className="chat-bubble-content">{m.content}</div>
                </div>
              ))}
              <div ref={bottomRef} />
            </div>
            <form className="chat-input-row" onSubmit={handleSend}>
              <input
                type="text"
                placeholder="Type a message… (use @file:C:\path\to\file.txt to attach a granted file)"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                disabled={sending}
              />
              <button type="submit" disabled={sending || !draft.trim()}>
                {sending ? "Sending…" : "Send"}
              </button>
            </form>
          </>
        )}
      </main>
    </div>
  );
}
