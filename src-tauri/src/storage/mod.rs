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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessGrant {
    pub id: String,
    pub agent_id: String,
    pub folder_path: String,
    pub granted_at: String,
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
            ",
        )
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

    pub fn list_agents(&self) -> rusqlite::Result<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, role_template, system_prompt, provider_kind, provider_name, model, created_at
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
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_agent(&self, id: &str) -> rusqlite::Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, role_template, system_prompt, provider_kind, provider_name, model, created_at
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
                    created_at: row.get(7)?,
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
            "INSERT OR IGNORE INTO session_agents (session_id, agent_id) VALUES (?1, ?2)",
            params![session_id, agent_id],
        )?;
        Ok(())
    }

    pub fn agents_for_session(&self, session_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT agent_id FROM session_agents WHERE session_id = ?1")?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        rows.collect()
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

    pub fn grant_folder_access(&self, agent_id: &str, folder_path: &str) -> rusqlite::Result<FileAccessGrant> {
        let grant = FileAccessGrant {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            folder_path: folder_path.to_string(),
            granted_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO file_access_grants (id, agent_id, folder_path, granted_at) VALUES (?1, ?2, ?3, ?4)",
            params![grant.id, grant.agent_id, grant.folder_path, grant.granted_at],
        )?;
        Ok(grant)
    }

    pub fn list_file_access_grants(&self, agent_id: &str) -> rusqlite::Result<Vec<FileAccessGrant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, folder_path, granted_at
             FROM file_access_grants WHERE agent_id = ?1 ORDER BY granted_at ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(FileAccessGrant {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                folder_path: row.get(2)?,
                granted_at: row.get(3)?,
            })
        })?;
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
