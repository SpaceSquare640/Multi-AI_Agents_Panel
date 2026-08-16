//! Storage: local-only SQLite persistence for conversations, agent
//! settings, and task-graph run logs. No cloud storage of any kind.
//! Design: `Multi-AI Agent Panel Document/01 Project Overview/Vision & Goals.md`

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// Name of the role template this agent was created from, if any
    /// (e.g. "Product Lead"). Purely informational — the actual behavior
    /// comes from `system_prompt`, which is copied in at creation time so
    /// editing/deleting the template later doesn't change existing agents.
    pub role_template: Option<String>,
    /// Sent to the provider as the system prompt (Anthropic: the top-level
    /// `system` field; others: a `role: "system"` message) — see
    /// `agent_manager::role_templates`.
    pub system_prompt: Option<String>,
    /// "local" | "cloud"
    pub provider_kind: String,
    /// e.g. "ollama" | "openai" | "anthropic" | "openrouter"
    pub provider_name: String,
    pub model: String,
    /// If set, `agent_manager::dispatch` uses exactly this Key Vault entry
    /// and does not fall back to other keys for the provider if it fails
    /// — pinning is an explicit override of the default "try the
    /// most-recently-added key for this provider, falling back through
    /// older ones" behavior (`storage::keys_for_provider`), e.g. so a
    /// specific agent's usage is tracked against a specific key. Set via
    /// `pin_agent_provider_key`, not at creation time, to keep
    /// `create_agent`'s signature stable.
    pub pinned_provider_key_id: Option<String>,
    pub created_at: String,
}

/// One step in an Agent's cross-provider fallback chain (Backlog: "跨
/// Provider 備援" — e.g. Anthropic fails, fall through to OpenRouter),
/// tried in `position` order only after the Agent's own primary
/// provider has exhausted its own key rotation. Key selection for a
/// fallback step always uses the full key rotation for that provider —
/// `Agent::pinned_provider_key_id` only applies to the Agent's primary
/// provider, since a pin is scoped to one specific provider/key pairing
/// and has no natural meaning for a provider the Agent isn't primarily
/// configured for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFallbackProvider {
    pub id: String,
    pub agent_id: String,
    pub position: i64,
    /// "local" | "cloud"
    pub provider_kind: String,
    pub provider_name: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// "independent" | "group"
    pub kind: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    /// None when the message is from the user.
    pub agent_id: Option<String>,
    /// "user" | "assistant" | "system"
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// Metadata for one entry in the Key Vault. The actual secret lives in the
/// OS credential store (`key_vault`), addressed by `id` — this row is only
/// ever the non-secret index: which provider it's for, an optional label
/// (e.g. "Ling-3.0-flash (free)"), and usage bookkeeping. A provider can
/// have more than one key (e.g. several free OpenRouter keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKey {
    pub id: String,
    /// e.g. "anthropic" | "openai" | "openrouter"
    pub provider: String,
    pub label: Option<String>,
    /// Optional: the specific model this key is scoped/intended for.
    pub model_hint: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// Aggregated call counts for one Key Vault entry, joined with its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub provider_key_id: String,
    pub provider: String,
    pub label: Option<String>,
    pub success_count: i64,
    pub failure_count: i64,
    pub last_used_at: Option<String>,
}

/// One folder an agent has been explicitly granted read access to.
/// Created only after the user picks the folder via the OS's native
/// folder picker — see `file_access` module docs.
///
/// `session_id` implements `Orchestration Design.md`'s decided "同場會議
/// 共用" rule: `None` means a private grant (only `agent_id` can use it,
/// in any session); `Some(session_id)` means every agent *currently* in
/// that Group Chat session can use it, not just `agent_id` — see
/// `storage::effective_granted_folders`, which is what authorization
/// checks (`file_access::read_file`, `list_text_files_in_grants`) query
/// against, not this raw per-row list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessGrant {
    pub id: String,
    pub agent_id: String,
    pub folder_path: String,
    pub granted_at: String,
    pub session_id: Option<String>,
}

/// One Skill an agent has been explicitly granted permission to call.
/// Mirrors `FileAccessGrant`'s consent model: granting is a deliberate,
/// separate step from an agent existing — see `skill_manager` module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAccessGrant {
    pub id: String,
    pub agent_id: String,
    pub skill_name: String,
    pub granted_at: String,
}

/// Grants access to one `ml_engine` capability (e.g. `semantic_search`).
/// Unlike `SkillAccessGrant`, this has a scope rather than always being
/// tied to one agent — see `ML Engine Design.md` in the vault ("同場會議
/// 共用"): a `session` grant is shared by every agent currently in that
/// Group Chat, not just whoever the grant command was called for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MlAccessGrant {
    pub id: String,
    /// "agent" | "session"
    pub scope_kind: String,
    /// An agent id when `scope_kind == "agent"`, a session id when
    /// `scope_kind == "session"`.
    pub scope_id: String,
    pub capability_name: String,
    pub granted_at: String,
}

/// A user-authored role template ("User Custom" in
/// `Role Templates Index.md` — as opposed to the 10 built-in "Default"
/// ones, which live in `agent_manager::role_templates` as Rust constants
/// and are never stored here, so an app update can safely refresh them).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRoleTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub suggested_provider_kind: Option<String>,
    pub suggested_provider_name: Option<String>,
    pub suggested_model: Option<String>,
    pub created_at: String,
}

/// Per-Group-Chat-session turn-taking state. `rotation_cursor` indexes into
/// the session's members (ordered by `joined_at`) for round-robin
/// speaking order; an `@mention` speaks out of turn without consuming a
/// rotation slot. `consecutive_agent_turns` is the mechanical loop
/// safety-net behind Error Code Registry E6001 — see `orchestrator`
/// module docs for why it's a turn cap rather than real disagreement
/// detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSessionState {
    pub session_id: String,
    pub rotation_cursor: i64,
    pub consecutive_agent_turns: i64,
}

pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Storage {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Only ever used by tests across the crate (`Storage::open_in_memory()`
    /// in every module's `#[cfg(test)] mod tests`) — gated the same way so
    /// a normal `cargo build` doesn't warn about it as dead code.
    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Storage {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                id             TEXT PRIMARY KEY,
                name           TEXT NOT NULL,
                role_template  TEXT,
                system_prompt  TEXT,
                provider_kind  TEXT NOT NULL,
                provider_name  TEXT NOT NULL,
                model          TEXT NOT NULL,
                created_at     TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS role_templates_custom (
                id                      TEXT PRIMARY KEY,
                name                    TEXT NOT NULL,
                description             TEXT NOT NULL,
                system_prompt           TEXT NOT NULL,
                suggested_provider_kind TEXT,
                suggested_provider_name TEXT,
                suggested_model         TEXT,
                created_at              TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                kind        TEXT NOT NULL,
                title       TEXT NOT NULL,
                created_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS session_agents (
                session_id  TEXT NOT NULL REFERENCES sessions(id),
                agent_id    TEXT NOT NULL REFERENCES agents(id),
                PRIMARY KEY (session_id, agent_id)
            );

            CREATE TABLE IF NOT EXISTS group_session_state (
                session_id              TEXT PRIMARY KEY REFERENCES sessions(id),
                rotation_cursor         INTEGER NOT NULL DEFAULT 0,
                consecutive_agent_turns INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS messages (
                id          TEXT PRIMARY KEY,
                session_id  TEXT NOT NULL REFERENCES sessions(id),
                agent_id    TEXT REFERENCES agents(id),
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                created_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS provider_keys (
                id            TEXT PRIMARY KEY,
                provider      TEXT NOT NULL,
                label         TEXT,
                model_hint    TEXT,
                created_at    TEXT NOT NULL,
                last_used_at  TEXT
            );

            CREATE TABLE IF NOT EXISTS usage_log (
                id                TEXT PRIMARY KEY,
                provider_key_id   TEXT REFERENCES provider_keys(id),
                agent_id          TEXT REFERENCES agents(id),
                provider          TEXT NOT NULL,
                model             TEXT NOT NULL,
                success           INTEGER NOT NULL,
                created_at        TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS file_access_grants (
                id            TEXT PRIMARY KEY,
                agent_id      TEXT NOT NULL REFERENCES agents(id),
                folder_path   TEXT NOT NULL,
                granted_at    TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS skill_access_grants (
                id            TEXT PRIMARY KEY,
                agent_id      TEXT NOT NULL REFERENCES agents(id),
                skill_name    TEXT NOT NULL,
                granted_at    TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ml_access_grants (
                id                TEXT PRIMARY KEY,
                scope_kind        TEXT NOT NULL,
                scope_id          TEXT NOT NULL,
                capability_name   TEXT NOT NULL,
                granted_at        TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_fallback_providers (
                id             TEXT PRIMARY KEY,
                agent_id       TEXT NOT NULL REFERENCES agents(id),
                position       INTEGER NOT NULL,
                provider_kind  TEXT NOT NULL,
                provider_name  TEXT NOT NULL,
                model          TEXT NOT NULL,
                created_at     TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS group_boundary_consent (
                session_id  TEXT PRIMARY KEY REFERENCES sessions(id),
                granted_at  TEXT NOT NULL
            );
            ",
        )?;
        // `session_agents` predates Group Chat's need for a stable join
        // order (round-robin speaking order); add the column for
        // pre-existing local databases rather than requiring a fresh one.
        Self::ensure_column(&conn, "session_agents", "joined_at", "joined_at TEXT")?;
        // `agents` predates key pinning; same soft-migration approach.
        Self::ensure_column(&conn, "agents", "pinned_provider_key_id", "pinned_provider_key_id TEXT")?;
        // `file_access_grants` predates Group Chat's "同場會議共用" rule.
        Self::ensure_column(&conn, "file_access_grants", "session_id", "session_id TEXT")
    }

    fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let existing: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<_>>()?;
        if !existing.iter().any(|c| c == column) {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {ddl}"), [])?;
        }
        Ok(())
    }

    pub fn create_agent(
        &self,
        name: &str,
        role_template: Option<&str>,
        system_prompt: Option<&str>,
        provider_kind: &str,
        provider_name: &str,
        model: &str,
    ) -> rusqlite::Result<Agent> {
        let agent = Agent {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            role_template: role_template.map(|s| s.to_string()),
            system_prompt: system_prompt.map(|s| s.to_string()),
            provider_kind: provider_kind.to_string(),
            provider_name: provider_name.to_string(),
            model: model.to_string(),
            pinned_provider_key_id: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO agents (id, name, role_template, system_prompt, provider_kind, provider_name, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                agent.id,
                agent.name,
                agent.role_template,
                agent.system_prompt,
                agent.provider_kind,
                agent.provider_name,
                agent.model,
                agent.created_at,
            ],
        )?;
        Ok(agent)
    }

    /// Pins (or, with `None`, un-pins) which Key Vault entry this agent
    /// uses for cloud calls — see `Agent::pinned_provider_key_id`.
    pub fn pin_agent_provider_key(&self, agent_id: &str, provider_key_id: Option<&str>) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE agents SET pinned_provider_key_id = ?1 WHERE id = ?2",
            params![provider_key_id, agent_id],
        )?;
        Ok(())
    }

    /// Appends one step to an Agent's cross-provider fallback chain —
    /// always at the end (`position` = current max + 1), so the order
    /// existing steps were added in is preserved.
    pub fn add_agent_fallback_provider(
        &self,
        agent_id: &str,
        provider_kind: &str,
        provider_name: &str,
        model: &str,
    ) -> rusqlite::Result<AgentFallbackProvider> {
        let conn = self.conn.lock().unwrap();
        let next_position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM agent_fallback_providers WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )?;
        let step = AgentFallbackProvider {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            position: next_position,
            provider_kind: provider_kind.to_string(),
            provider_name: provider_name.to_string(),
            model: model.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        conn.execute(
            "INSERT INTO agent_fallback_providers (id, agent_id, position, provider_kind, provider_name, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                step.id,
                step.agent_id,
                step.position,
                step.provider_kind,
                step.provider_name,
                step.model,
                step.created_at,
            ],
        )?;
        Ok(step)
    }

    /// The fallback chain for one Agent, in the order `agent_manager::dispatch`
    /// should try them — after the Agent's own primary provider, before
    /// giving up with E3001.
    pub fn list_agent_fallback_providers(&self, agent_id: &str) -> rusqlite::Result<Vec<AgentFallbackProvider>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, position, provider_kind, provider_name, model, created_at
             FROM agent_fallback_providers WHERE agent_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(AgentFallbackProvider {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                position: row.get(2)?,
                provider_kind: row.get(3)?,
                provider_name: row.get(4)?,
                model: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn remove_agent_fallback_provider(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM agent_fallback_providers WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_provider_key(&self, id: &str) -> rusqlite::Result<Option<ProviderKey>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, provider, label, model_hint, created_at, last_used_at FROM provider_keys WHERE id = ?1",
            params![id],
            |row| {
                Ok(ProviderKey {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    label: row.get(2)?,
                    model_hint: row.get(3)?,
                    created_at: row.get(4)?,
                    last_used_at: row.get(5)?,
                })
            },
        )
        .optional()
    }

    pub fn list_agents(&self) -> rusqlite::Result<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, role_template, system_prompt, provider_kind, provider_name, model, pinned_provider_key_id, created_at
             FROM agents ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                role_template: row.get(2)?,
                system_prompt: row.get(3)?,
                provider_kind: row.get(4)?,
                provider_name: row.get(5)?,
                model: row.get(6)?,
                pinned_provider_key_id: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_agent(&self, id: &str) -> rusqlite::Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, role_template, system_prompt, provider_kind, provider_name, model, pinned_provider_key_id, created_at
             FROM agents WHERE id = ?1",
            params![id],
            |row| {
                Ok(Agent {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    role_template: row.get(2)?,
                    system_prompt: row.get(3)?,
                    provider_kind: row.get(4)?,
                    provider_name: row.get(5)?,
                    model: row.get(6)?,
                    pinned_provider_key_id: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .optional()
    }

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, title, created_at FROM sessions ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                kind: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub fn create_session(&self, kind: &str, title: &str) -> rusqlite::Result<Session> {
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO sessions (id, kind, title, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session.id, session.kind, session.title, session.created_at],
        )?;
        Ok(session)
    }

    pub fn add_agent_to_session(&self, session_id: &str, agent_id: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO session_agents (session_id, agent_id, joined_at) VALUES (?1, ?2, ?3)",
            params![session_id, agent_id, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Members of a session, ordered by when they joined — for an
    /// Independent Session this is just the one agent; for a Group Chat
    /// it's the round-robin speaking order (`session_manager`).
    pub fn agents_for_session(&self, session_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT agent_id FROM session_agents WHERE session_id = ?1 ORDER BY joined_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    /// Reads (creating with defaults if absent) a Group Chat session's
    /// turn-taking state.
    pub fn get_group_session_state(&self, session_id: &str) -> rusqlite::Result<GroupSessionState> {
        let conn = self.conn.lock().unwrap();
        let existing = conn
            .query_row(
                "SELECT session_id, rotation_cursor, consecutive_agent_turns FROM group_session_state WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok(GroupSessionState {
                        session_id: row.get(0)?,
                        rotation_cursor: row.get(1)?,
                        consecutive_agent_turns: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if let Some(state) = existing {
            return Ok(state);
        }
        conn.execute(
            "INSERT INTO group_session_state (session_id, rotation_cursor, consecutive_agent_turns) VALUES (?1, 0, 0)",
            params![session_id],
        )?;
        Ok(GroupSessionState { session_id: session_id.to_string(), rotation_cursor: 0, consecutive_agent_turns: 0 })
    }

    pub fn save_group_session_state(&self, state: &GroupSessionState) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO group_session_state (session_id, rotation_cursor, consecutive_agent_turns)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET rotation_cursor = ?2, consecutive_agent_turns = ?3",
            params![state.session_id, state.rotation_cursor, state.consecutive_agent_turns],
        )?;
        Ok(())
    }

    /// Called whenever a real user message lands in a Group Chat — the
    /// loop safety-net (E6001) only cares about *consecutive* agent turns
    /// with no user in between.
    pub fn reset_group_session_turn_counter(&self, session_id: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO group_session_state (session_id, rotation_cursor, consecutive_agent_turns)
             VALUES (?1, 0, 0)
             ON CONFLICT(session_id) DO UPDATE SET consecutive_agent_turns = 0",
            params![session_id],
        )?;
        Ok(())
    }

    /// Records that the user has confirmed sending local-Agent-produced
    /// content across the local→cloud boundary for this Group Chat
    /// session — see `Orchestration Design.md`'s decided "第一次跨越本地
    /// →雲端邊界時顯示即將送出的內容供使用者確認（可設定本場 Group Chat
    /// 都記住我的選擇）" rule. Idempotent: confirming twice for the same
    /// session is a no-op, not an error.
    pub fn grant_local_to_cloud_consent(&self, session_id: &str) -> rusqlite::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.lock().unwrap().execute(
            "INSERT INTO group_boundary_consent (session_id, granted_at) VALUES (?1, ?2)
             ON CONFLICT(session_id) DO NOTHING",
            params![session_id, now],
        )?;
        Ok(())
    }

    pub fn has_local_to_cloud_consent(&self, session_id: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM group_boundary_consent WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn add_message(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
        role: &str,
        content: &str,
    ) -> rusqlite::Result<Message> {
        let message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            agent_id: agent_id.map(|s| s.to_string()),
            role: role.to_string(),
            content: content.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO messages (id, session_id, agent_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                message.id,
                message.session_id,
                message.agent_id,
                message.role,
                message.content,
                message.created_at,
            ],
        )?;
        Ok(message)
    }

    pub fn list_messages(&self, session_id: &str) -> rusqlite::Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, agent_id, role, content, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Indexes a new Key Vault entry. Does **not** touch the secret itself
    /// — callers are expected to also call `key_vault::set_secret(&id, ...)`
    /// with the returned id.
    pub fn create_provider_key(
        &self,
        provider: &str,
        label: Option<&str>,
        model_hint: Option<&str>,
    ) -> rusqlite::Result<ProviderKey> {
        let key = ProviderKey {
            id: uuid::Uuid::new_v4().to_string(),
            provider: provider.to_string(),
            label: label.map(str::to_string),
            model_hint: model_hint.map(str::to_string),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_used_at: None,
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO provider_keys (id, provider, label, model_hint, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![key.id, key.provider, key.label, key.model_hint, key.created_at],
        )?;
        Ok(key)
    }

    pub fn list_provider_keys(&self) -> rusqlite::Result<Vec<ProviderKey>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, label, model_hint, created_at, last_used_at
             FROM provider_keys ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProviderKey {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                model_hint: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// The most recently created key for a provider — used as the default
    /// when an agent doesn't explicitly pin a specific key entry.
    pub fn latest_provider_key(&self, provider: &str) -> rusqlite::Result<Option<ProviderKey>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, provider, label, model_hint, created_at, last_used_at
             FROM provider_keys WHERE provider = ?1 ORDER BY created_at DESC LIMIT 1",
            params![provider],
            |row| {
                Ok(ProviderKey {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    label: row.get(2)?,
                    model_hint: row.get(3)?,
                    created_at: row.get(4)?,
                    last_used_at: row.get(5)?,
                })
            },
        )
        .optional()
    }

    /// All keys for a provider, most recently added first — the order
    /// `agent_manager`'s fallback chain tries them in.
    pub fn keys_for_provider(&self, provider: &str) -> rusqlite::Result<Vec<ProviderKey>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, label, model_hint, created_at, last_used_at
             FROM provider_keys WHERE provider = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![provider], |row| {
            Ok(ProviderKey {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                model_hint: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Deletes the metadata row only — callers should also call
    /// `key_vault::delete_secret(id)`.
    pub fn delete_provider_key(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM provider_keys WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Records one provider call (success or failure) and bumps the key's
    /// `last_used_at`, if a specific key was used.
    pub fn record_usage(
        &self,
        provider_key_id: Option<&str>,
        agent_id: Option<&str>,
        provider: &str,
        model: &str,
        success: bool,
    ) -> rusqlite::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO usage_log (id, provider_key_id, agent_id, provider, model, success, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                provider_key_id,
                agent_id,
                provider,
                model,
                success as i64,
                now,
            ],
        )?;
        if let Some(id) = provider_key_id {
            conn.execute(
                "UPDATE provider_keys SET last_used_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
        }
        Ok(())
    }

    pub fn usage_summary(&self) -> rusqlite::Result<Vec<UsageSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                pk.id,
                pk.provider,
                pk.label,
                COALESCE(SUM(CASE WHEN u.success = 1 THEN 1 ELSE 0 END), 0) AS success_count,
                COALESCE(SUM(CASE WHEN u.success = 0 THEN 1 ELSE 0 END), 0) AS failure_count,
                pk.last_used_at
             FROM provider_keys pk
             LEFT JOIN usage_log u ON u.provider_key_id = pk.id
             GROUP BY pk.id
             ORDER BY pk.created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UsageSummary {
                provider_key_id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                success_count: row.get(3)?,
                failure_count: row.get(4)?,
                last_used_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Grants a private folder to `agent_id` alone. Use
    /// `grant_folder_access_for_session` instead for Group Chat's shared
    /// grants.
    pub fn grant_folder_access(&self, agent_id: &str, folder_path: &str) -> rusqlite::Result<FileAccessGrant> {
        self.insert_file_access_grant(agent_id, folder_path, None)
    }

    /// Grants a folder shared by every agent *currently* in `session_id`
    /// (a Group Chat), per `Orchestration Design.md`'s "同場會議共用"
    /// rule. `agent_id` records who picked the folder (shown in the UI)
    /// but is not itself special — `effective_granted_folders` checks
    /// live session membership, not this row's `agent_id`.
    pub fn grant_folder_access_for_session(
        &self,
        session_id: &str,
        agent_id: &str,
        folder_path: &str,
    ) -> rusqlite::Result<FileAccessGrant> {
        self.insert_file_access_grant(agent_id, folder_path, Some(session_id))
    }

    fn insert_file_access_grant(
        &self,
        agent_id: &str,
        folder_path: &str,
        session_id: Option<&str>,
    ) -> rusqlite::Result<FileAccessGrant> {
        let grant = FileAccessGrant {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            folder_path: folder_path.to_string(),
            granted_at: chrono::Utc::now().to_rfc3339(),
            session_id: session_id.map(str::to_string),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO file_access_grants (id, agent_id, folder_path, granted_at, session_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![grant.id, grant.agent_id, grant.folder_path, grant.granted_at, grant.session_id],
        )?;
        Ok(grant)
    }

    /// Every grant recorded under `agent_id` — private ones and ones this
    /// agent shared to a session it granted from. This is what the UI
    /// displays/revokes; for authorization decisions use
    /// `effective_granted_folders` instead, which also includes grants
    /// *other* agents shared to a session `agent_id` is currently in.
    pub fn list_file_access_grants(&self, agent_id: &str) -> rusqlite::Result<Vec<FileAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, folder_path, granted_at, session_id
             FROM file_access_grants WHERE agent_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(FileAccessGrant {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                folder_path: row.get(2)?,
                granted_at: row.get(3)?,
                session_id: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Every grant shared to `session_id` (by any of its members) — for
    /// a Group Chat tab's "Files" display, which shows the whole
    /// meeting's shared folders rather than one agent's private list.
    pub fn list_session_shared_file_grants(&self, session_id: &str) -> rusqlite::Result<Vec<FileAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, folder_path, granted_at, session_id
             FROM file_access_grants WHERE session_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(FileAccessGrant {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                folder_path: row.get(2)?,
                granted_at: row.get(3)?,
                session_id: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// The actual set of folders `agent_id` may read from right now:
    /// its own private grants, plus every folder shared to a Group Chat
    /// session it is *currently* a member of (regardless of which agent
    /// originally granted it). This — not `list_file_access_grants` — is
    /// what `file_access::read_file` and `list_text_files_in_grants`
    /// check against.
    pub fn effective_granted_folders(&self, agent_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT folder_path FROM file_access_grants WHERE agent_id = ?1 AND session_id IS NULL
             UNION
             SELECT g.folder_path FROM file_access_grants g
             JOIN session_agents sa ON sa.session_id = g.session_id
             WHERE g.session_id IS NOT NULL AND sa.agent_id = ?1",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn revoke_file_access_grant(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM file_access_grants WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn grant_skill_access(&self, agent_id: &str, skill_name: &str) -> rusqlite::Result<SkillAccessGrant> {
        let grant = SkillAccessGrant {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            skill_name: skill_name.to_string(),
            granted_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO skill_access_grants (id, agent_id, skill_name, granted_at) VALUES (?1, ?2, ?3, ?4)",
            params![grant.id, grant.agent_id, grant.skill_name, grant.granted_at],
        )?;
        Ok(grant)
    }

    pub fn list_skill_access_grants(&self, agent_id: &str) -> rusqlite::Result<Vec<SkillAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, skill_name, granted_at
             FROM skill_access_grants WHERE agent_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(SkillAccessGrant {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                skill_name: row.get(2)?,
                granted_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub fn revoke_skill_access_grant(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM skill_access_grants WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// `scope_kind` must be `"agent"` or `"session"` — see `MlAccessGrant`
    /// docs. Callers (`commands::grant_ml_capability_to_agent` /
    /// `..._to_session`) are responsible for picking the right one; this
    /// method doesn't validate the value itself, same as how
    /// `create_agent`'s `provider_kind` string isn't validated here.
    pub fn grant_ml_capability(
        &self,
        scope_kind: &str,
        scope_id: &str,
        capability_name: &str,
    ) -> rusqlite::Result<MlAccessGrant> {
        let grant = MlAccessGrant {
            id: uuid::Uuid::new_v4().to_string(),
            scope_kind: scope_kind.to_string(),
            scope_id: scope_id.to_string(),
            capability_name: capability_name.to_string(),
            granted_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO ml_access_grants (id, scope_kind, scope_id, capability_name, granted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![grant.id, grant.scope_kind, grant.scope_id, grant.capability_name, grant.granted_at],
        )?;
        Ok(grant)
    }

    pub fn revoke_ml_access_grant(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM ml_access_grants WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// All grants directly on `agent_id` (`scope_kind = "agent"`) — does
    /// **not** include session-shared grants; use
    /// `has_ml_capability_access` to check actual effective access
    /// (which does include those).
    pub fn list_ml_access_grants_for_agent(&self, agent_id: &str) -> rusqlite::Result<Vec<MlAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, scope_kind, scope_id, capability_name, granted_at
             FROM ml_access_grants WHERE scope_kind = 'agent' AND scope_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(MlAccessGrant {
                id: row.get(0)?,
                scope_kind: row.get(1)?,
                scope_id: row.get(2)?,
                capability_name: row.get(3)?,
                granted_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_ml_access_grants_for_session(&self, session_id: &str) -> rusqlite::Result<Vec<MlAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, scope_kind, scope_id, capability_name, granted_at
             FROM ml_access_grants WHERE scope_kind = 'session' AND scope_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(MlAccessGrant {
                id: row.get(0)?,
                scope_kind: row.get(1)?,
                scope_id: row.get(2)?,
                capability_name: row.get(3)?,
                granted_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Whether `agent_id` may call `capability_name` — true if the agent
    /// has a direct `"agent"`-scope grant, **or** if any Group Chat
    /// session it's currently a member of has a `"session"`-scope grant
    /// for that capability (the "同場會議共用" rule from `ML Engine
    /// Design.md`). Every session-scope grant is checked against the
    /// agent's *current* membership, not membership at grant time, so
    /// leaving a Group Chat also revokes the shared access.
    pub fn has_ml_capability_access(&self, agent_id: &str, capability_name: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let direct: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ml_access_grants WHERE scope_kind = 'agent' AND scope_id = ?1 AND capability_name = ?2",
            params![agent_id, capability_name],
            |row| row.get(0),
        )?;
        if direct > 0 {
            return Ok(true);
        }

        let mut stmt = conn.prepare(
            "SELECT DISTINCT scope_id FROM ml_access_grants WHERE scope_kind = 'session' AND capability_name = ?1",
        )?;
        let session_ids: Vec<String> = stmt.query_map(params![capability_name], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        for session_id in session_ids {
            let is_member: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_agents WHERE session_id = ?1 AND agent_id = ?2",
                params![session_id, agent_id],
                |row| row.get(0),
            )?;
            if is_member > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_custom_role_template(
        &self,
        name: &str,
        description: &str,
        system_prompt: &str,
        suggested_provider_kind: Option<&str>,
        suggested_provider_name: Option<&str>,
        suggested_model: Option<&str>,
    ) -> rusqlite::Result<CustomRoleTemplate> {
        let template = CustomRoleTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            system_prompt: system_prompt.to_string(),
            suggested_provider_kind: suggested_provider_kind.map(str::to_string),
            suggested_provider_name: suggested_provider_name.map(str::to_string),
            suggested_model: suggested_model.map(str::to_string),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO role_templates_custom
                (id, name, description, system_prompt, suggested_provider_kind, suggested_provider_name, suggested_model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                template.id,
                template.name,
                template.description,
                template.system_prompt,
                template.suggested_provider_kind,
                template.suggested_provider_name,
                template.suggested_model,
                template.created_at,
            ],
        )?;
        Ok(template)
    }

    pub fn list_custom_role_templates(&self) -> rusqlite::Result<Vec<CustomRoleTemplate>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, system_prompt, suggested_provider_kind, suggested_provider_name, suggested_model, created_at
             FROM role_templates_custom ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CustomRoleTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                system_prompt: row.get(3)?,
                suggested_provider_kind: row.get(4)?,
                suggested_provider_name: row.get(5)?,
                suggested_model: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Edits an existing custom template in place — same row, same `id`
    /// and `created_at`, only the content changes. Agents created from
    /// this template *before* the edit are unaffected (their
    /// `system_prompt` was copied at creation time, see `Agent` docs);
    /// this only changes what a *future* "apply this template" picks up.
    #[allow(clippy::too_many_arguments)]
    pub fn update_custom_role_template(
        &self,
        id: &str,
        name: &str,
        description: &str,
        system_prompt: &str,
        suggested_provider_kind: Option<&str>,
        suggested_provider_name: Option<&str>,
        suggested_model: Option<&str>,
    ) -> rusqlite::Result<CustomRoleTemplate> {
        self.conn.lock().unwrap().execute(
            "UPDATE role_templates_custom
             SET name = ?2, description = ?3, system_prompt = ?4,
                 suggested_provider_kind = ?5, suggested_provider_name = ?6, suggested_model = ?7
             WHERE id = ?1",
            params![id, name, description, system_prompt, suggested_provider_kind, suggested_provider_name, suggested_model],
        )?;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, description, system_prompt, suggested_provider_kind, suggested_provider_name, suggested_model, created_at
             FROM role_templates_custom WHERE id = ?1",
            params![id],
            |row| {
                Ok(CustomRoleTemplate {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    system_prompt: row.get(3)?,
                    suggested_provider_kind: row.get(4)?,
                    suggested_provider_name: row.get(5)?,
                    suggested_model: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
    }

    pub fn delete_custom_role_template(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM role_templates_custom WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list_agent() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage
            .create_agent("Full-Stack Developer", Some("Full-Stack Developer"), None, "cloud", "anthropic", "claude")
            .unwrap();

        let agents = storage.list_agents().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, agent.id);
        assert_eq!(agents[0].name, "Full-Stack Developer");

        let fetched = storage.get_agent(&agent.id).unwrap().unwrap();
        assert_eq!(fetched.name, "Full-Stack Developer");
        assert!(storage.get_agent("does-not-exist").unwrap().is_none());
    }

    #[test]
    fn list_sessions_returns_created_sessions() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.list_sessions().unwrap().is_empty());
        let session = storage.create_session("independent", "Test session").unwrap();
        let sessions = storage.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session.id);
    }

    #[test]
    fn session_and_messages_round_trip() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage
            .create_agent("Local Agent", None, None, "local", "ollama", "llama3.1:8b")
            .unwrap();
        let session = storage.create_session("independent", "Test session").unwrap();
        storage.add_agent_to_session(&session.id, &agent.id).unwrap();
        assert_eq!(storage.agents_for_session(&session.id).unwrap(), vec![agent.id.clone()]);

        storage
            .add_message(&session.id, None, "user", "hello")
            .unwrap();
        storage
            .add_message(&session.id, Some(&agent.id), "assistant", "hi there")
            .unwrap();

        let messages = storage.list_messages(&session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].agent_id.as_deref(), Some(agent.id.as_str()));
    }

    #[test]
    fn empty_db_has_no_agents() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.list_agents().unwrap().is_empty());
    }

    #[test]
    fn file_access_grant_crud() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        assert!(storage.list_file_access_grants(&agent.id).unwrap().is_empty());

        let grant = storage.grant_folder_access(&agent.id, "/tmp/notes").unwrap();
        let grants = storage.list_file_access_grants(&agent.id).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].folder_path, "/tmp/notes");

        storage.revoke_file_access_grant(&grant.id).unwrap();
        assert!(storage.list_file_access_grants(&agent.id).unwrap().is_empty());
    }

    #[test]
    fn private_grant_is_only_effective_for_its_own_agent() {
        let storage = Storage::open_in_memory().unwrap();
        let owner = storage.create_agent("Owner", None, None, "cloud", "anthropic", "claude").unwrap();
        let other = storage.create_agent("Other", None, None, "cloud", "anthropic", "claude").unwrap();
        storage.grant_folder_access(&owner.id, "/tmp/notes").unwrap();

        assert_eq!(storage.effective_granted_folders(&owner.id).unwrap(), vec!["/tmp/notes".to_string()]);
        assert!(storage.effective_granted_folders(&other.id).unwrap().is_empty());
    }

    #[test]
    fn session_shared_grant_is_effective_for_every_current_member() {
        let storage = Storage::open_in_memory().unwrap();
        let a = storage.create_agent("A", None, None, "cloud", "anthropic", "claude").unwrap();
        let b = storage.create_agent("B", None, None, "cloud", "anthropic", "claude").unwrap();
        let outsider = storage.create_agent("Outsider", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();
        storage.add_agent_to_session(&session.id, &a.id).unwrap();
        storage.add_agent_to_session(&session.id, &b.id).unwrap();

        // A picks the folder, but it should be usable by every member —
        // not just A.
        storage.grant_folder_access_for_session(&session.id, &a.id, "/tmp/shared").unwrap();

        assert_eq!(storage.effective_granted_folders(&a.id).unwrap(), vec!["/tmp/shared".to_string()]);
        assert_eq!(storage.effective_granted_folders(&b.id).unwrap(), vec!["/tmp/shared".to_string()]);
        assert!(storage.effective_granted_folders(&outsider.id).unwrap().is_empty());

        let shared = storage.list_session_shared_file_grants(&session.id).unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].folder_path, "/tmp/shared");
    }

    #[test]
    fn session_shared_grant_checks_current_membership_not_a_snapshot() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("A", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();

        storage.grant_folder_access_for_session(&session.id, &agent.id, "/tmp/shared").unwrap();
        // Not a member yet (grant_folder_access_for_session doesn't add
        // membership on its own) — should not be effective.
        assert!(storage.effective_granted_folders(&agent.id).unwrap().is_empty());

        storage.add_agent_to_session(&session.id, &agent.id).unwrap();
        assert_eq!(storage.effective_granted_folders(&agent.id).unwrap(), vec!["/tmp/shared".to_string()]);
    }

    #[test]
    fn agent_key_pin_round_trips_and_can_be_cleared() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "openrouter", "model").unwrap();
        assert_eq!(agent.pinned_provider_key_id, None);

        let key = storage.create_provider_key("openrouter", Some("pinned"), None).unwrap();
        storage.pin_agent_provider_key(&agent.id, Some(&key.id)).unwrap();
        let fetched = storage.get_agent(&agent.id).unwrap().unwrap();
        assert_eq!(fetched.pinned_provider_key_id.as_deref(), Some(key.id.as_str()));

        storage.pin_agent_provider_key(&agent.id, None).unwrap();
        let cleared = storage.get_agent(&agent.id).unwrap().unwrap();
        assert_eq!(cleared.pinned_provider_key_id, None);
    }

    #[test]
    fn agent_fallback_chain_preserves_add_order_and_can_be_shortened() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();

        let first = storage.add_agent_fallback_provider(&agent.id, "cloud", "openrouter", "some/model").unwrap();
        let second = storage.add_agent_fallback_provider(&agent.id, "local", "ollama", "llama3.1:8b").unwrap();
        assert_eq!(first.position, 0);
        assert_eq!(second.position, 1);

        let chain = storage.list_agent_fallback_providers(&agent.id).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, first.id);
        assert_eq!(chain[1].id, second.id);

        storage.remove_agent_fallback_provider(&first.id).unwrap();
        let remaining = storage.list_agent_fallback_providers(&agent.id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.id);
    }

    #[test]
    fn agent_fallback_chain_is_empty_for_an_agent_with_none_added() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        assert!(storage.list_agent_fallback_providers(&agent.id).unwrap().is_empty());
    }

    #[test]
    fn agents_for_session_preserves_join_order() {
        let storage = Storage::open_in_memory().unwrap();
        let a = storage.create_agent("A", None, None, "cloud", "anthropic", "claude").unwrap();
        let b = storage.create_agent("B", None, None, "cloud", "anthropic", "claude").unwrap();
        let c = storage.create_agent("C", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();
        storage.add_agent_to_session(&session.id, &b.id).unwrap();
        storage.add_agent_to_session(&session.id, &a.id).unwrap();
        storage.add_agent_to_session(&session.id, &c.id).unwrap();

        assert_eq!(storage.agents_for_session(&session.id).unwrap(), vec![b.id, a.id, c.id]);
    }

    #[test]
    fn group_session_state_defaults_then_persists_updates() {
        let storage = Storage::open_in_memory().unwrap();
        let session = storage.create_session("group", "Standup").unwrap();

        let state = storage.get_group_session_state(&session.id).unwrap();
        assert_eq!(state.rotation_cursor, 0);
        assert_eq!(state.consecutive_agent_turns, 0);

        storage
            .save_group_session_state(&GroupSessionState {
                session_id: session.id.clone(),
                rotation_cursor: 2,
                consecutive_agent_turns: 4,
            })
            .unwrap();
        let reloaded = storage.get_group_session_state(&session.id).unwrap();
        assert_eq!(reloaded.rotation_cursor, 2);
        assert_eq!(reloaded.consecutive_agent_turns, 4);

        storage.reset_group_session_turn_counter(&session.id).unwrap();
        let after_reset = storage.get_group_session_state(&session.id).unwrap();
        assert_eq!(after_reset.consecutive_agent_turns, 0);
        assert_eq!(after_reset.rotation_cursor, 2, "resetting the turn counter must not touch the rotation cursor");
    }

    #[test]
    fn local_to_cloud_consent_defaults_to_false_and_is_idempotent_once_granted() {
        let storage = Storage::open_in_memory().unwrap();
        let session = storage.create_session("group", "Standup").unwrap();

        assert!(!storage.has_local_to_cloud_consent(&session.id).unwrap());

        storage.grant_local_to_cloud_consent(&session.id).unwrap();
        assert!(storage.has_local_to_cloud_consent(&session.id).unwrap());

        // Granting twice must not error (idempotent, not a duplicate-key failure).
        storage.grant_local_to_cloud_consent(&session.id).unwrap();
        assert!(storage.has_local_to_cloud_consent(&session.id).unwrap());
    }

    #[test]
    fn local_to_cloud_consent_is_scoped_to_its_own_session() {
        let storage = Storage::open_in_memory().unwrap();
        let session_a = storage.create_session("group", "A").unwrap();
        let session_b = storage.create_session("group", "B").unwrap();

        storage.grant_local_to_cloud_consent(&session_a.id).unwrap();
        assert!(storage.has_local_to_cloud_consent(&session_a.id).unwrap());
        assert!(!storage.has_local_to_cloud_consent(&session_b.id).unwrap());
    }

    #[test]
    fn skill_access_grant_crud() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        assert!(storage.list_skill_access_grants(&agent.id).unwrap().is_empty());

        let grant = storage.grant_skill_access(&agent.id, "example_skill").unwrap();
        let grants = storage.list_skill_access_grants(&agent.id).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].skill_name, "example_skill");

        storage.revoke_skill_access_grant(&grant.id).unwrap();
        assert!(storage.list_skill_access_grants(&agent.id).unwrap().is_empty());
    }

    #[test]
    fn agent_scoped_ml_grant_only_authorizes_that_agent() {
        let storage = Storage::open_in_memory().unwrap();
        let granted = storage.create_agent("Granted", None, None, "cloud", "anthropic", "claude").unwrap();
        let other = storage.create_agent("Other", None, None, "cloud", "anthropic", "claude").unwrap();

        assert!(!storage.has_ml_capability_access(&granted.id, "semantic_search").unwrap());

        let grant = storage.grant_ml_capability("agent", &granted.id, "semantic_search").unwrap();
        assert!(storage.has_ml_capability_access(&granted.id, "semantic_search").unwrap());
        assert!(!storage.has_ml_capability_access(&other.id, "semantic_search").unwrap());

        let grants = storage.list_ml_access_grants_for_agent(&granted.id).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].id, grant.id);

        storage.revoke_ml_access_grant(&grant.id).unwrap();
        assert!(!storage.has_ml_capability_access(&granted.id, "semantic_search").unwrap());
    }

    #[test]
    fn session_scoped_ml_grant_is_shared_by_every_current_member() {
        let storage = Storage::open_in_memory().unwrap();
        let a = storage.create_agent("A", None, None, "cloud", "anthropic", "claude").unwrap();
        let b = storage.create_agent("B", None, None, "cloud", "anthropic", "claude").unwrap();
        let outsider = storage.create_agent("Outsider", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();
        storage.add_agent_to_session(&session.id, &a.id).unwrap();
        storage.add_agent_to_session(&session.id, &b.id).unwrap();

        storage.grant_ml_capability("session", &session.id, "semantic_search").unwrap();

        assert!(storage.has_ml_capability_access(&a.id, "semantic_search").unwrap());
        assert!(storage.has_ml_capability_access(&b.id, "semantic_search").unwrap());
        assert!(!storage.has_ml_capability_access(&outsider.id, "semantic_search").unwrap());

        let grants = storage.list_ml_access_grants_for_session(&session.id).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].scope_kind, "session");
    }

    #[test]
    fn leaving_has_no_way_to_happen_but_grant_checks_membership_live_not_at_grant_time() {
        // Regression guard for the design's stated invariant: session
        // grants are checked against *current* membership, not a
        // snapshot taken when the grant was created. There's no "leave
        // session" API yet, so this proves the query itself is
        // membership-driven by adding the grant before the agent joins.
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("A", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();

        storage.grant_ml_capability("session", &session.id, "semantic_search").unwrap();
        assert!(!storage.has_ml_capability_access(&agent.id, "semantic_search").unwrap());

        storage.add_agent_to_session(&session.id, &agent.id).unwrap();
        assert!(storage.has_ml_capability_access(&agent.id, "semantic_search").unwrap());
    }

    #[test]
    fn custom_role_template_crud() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.list_custom_role_templates().unwrap().is_empty());

        let template = storage
            .create_custom_role_template(
                "Release Notes Writer",
                "Turns a diff into a changelog entry",
                "You are a release notes writer. Summarize the change in one sentence.",
                Some("cloud"),
                Some("anthropic"),
                Some("claude-sonnet-4-5"),
            )
            .unwrap();

        let templates = storage.list_custom_role_templates().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].name, "Release Notes Writer");
        assert_eq!(templates[0].suggested_model.as_deref(), Some("claude-sonnet-4-5"));

        storage.delete_custom_role_template(&template.id).unwrap();
        assert!(storage.list_custom_role_templates().unwrap().is_empty());
    }

    #[test]
    fn update_custom_role_template_edits_in_place_keeping_the_same_id() {
        let storage = Storage::open_in_memory().unwrap();
        let template = storage
            .create_custom_role_template(
                "Draft Name",
                "Draft description",
                "Draft prompt",
                None,
                None,
                None,
            )
            .unwrap();

        storage
            .update_custom_role_template(
                &template.id,
                "Final Name",
                "Final description",
                "Final prompt",
                Some("cloud"),
                Some("anthropic"),
                Some("claude-sonnet-4-5"),
            )
            .unwrap();

        let templates = storage.list_custom_role_templates().unwrap();
        assert_eq!(templates.len(), 1, "editing must not create a second row");
        assert_eq!(templates[0].id, template.id, "editing must keep the same id");
        assert_eq!(templates[0].name, "Final Name");
        assert_eq!(templates[0].description, "Final description");
        assert_eq!(templates[0].system_prompt, "Final prompt");
        assert_eq!(templates[0].suggested_model.as_deref(), Some("claude-sonnet-4-5"));
    }

    #[test]
    fn agent_persists_its_system_prompt() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage
            .create_agent(
                "PM Bot",
                Some("Product Lead"),
                Some("You are the Product Lead."),
                "cloud",
                "anthropic",
                "claude-sonnet-4-5",
            )
            .unwrap();
        let fetched = storage.get_agent(&agent.id).unwrap().unwrap();
        assert_eq!(fetched.system_prompt.as_deref(), Some("You are the Product Lead."));
    }

    #[test]
    fn provider_key_crud_and_latest_lookup() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.latest_provider_key("openrouter").unwrap().is_none());

        let first = storage
            .create_provider_key("openrouter", Some("Ling-3.0-flash (free)"), Some("inclusionai/ling-3.0-flash:free"))
            .unwrap();
        let second = storage
            .create_provider_key("openrouter", Some("Poolside S (free)"), None)
            .unwrap();

        let keys = storage.list_provider_keys().unwrap();
        assert_eq!(keys.len(), 2);

        // "latest" is the most recently created one for that provider.
        let latest = storage.latest_provider_key("openrouter").unwrap().unwrap();
        assert_eq!(latest.id, second.id);

        // The fallback chain tries most-recently-added first.
        let ordered = storage.keys_for_provider("openrouter").unwrap();
        assert_eq!(ordered.iter().map(|k| &k.id).collect::<Vec<_>>(), vec![&second.id, &first.id]);

        storage.delete_provider_key(&first.id).unwrap();
        assert_eq!(storage.list_provider_keys().unwrap().len(), 1);
    }

    #[test]
    fn usage_summary_aggregates_success_and_failure() {
        let storage = Storage::open_in_memory().unwrap();
        let key = storage
            .create_provider_key("anthropic", Some("main"), None)
            .unwrap();

        storage
            .record_usage(Some(&key.id), None, "anthropic", "claude-sonnet", true)
            .unwrap();
        storage
            .record_usage(Some(&key.id), None, "anthropic", "claude-sonnet", true)
            .unwrap();
        storage
            .record_usage(Some(&key.id), None, "anthropic", "claude-sonnet", false)
            .unwrap();

        let summary = storage.usage_summary().unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].provider_key_id, key.id);
        assert_eq!(summary[0].success_count, 2);
        assert_eq!(summary[0].failure_count, 1);
        assert!(summary[0].last_used_at.is_some());

        let refreshed = storage.list_provider_keys().unwrap();
        assert!(refreshed[0].last_used_at.is_some());
    }
}
