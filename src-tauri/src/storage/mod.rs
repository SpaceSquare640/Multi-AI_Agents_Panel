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
    pub role_template: Option<String>,
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
                provider_kind  TEXT NOT NULL,
                provider_name  TEXT NOT NULL,
                model          TEXT NOT NULL,
                created_at     TEXT NOT NULL
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
            ",
        )
    }

    pub fn create_agent(
        &self,
        name: &str,
        role_template: Option<&str>,
        provider_kind: &str,
        provider_name: &str,
        model: &str,
    ) -> rusqlite::Result<Agent> {
        let agent = Agent {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            role_template: role_template.map(|s| s.to_string()),
            provider_kind: provider_kind.to_string(),
            provider_name: provider_name.to_string(),
            model: model.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO agents (id, name, role_template, provider_kind, provider_name, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                agent.id,
                agent.name,
                agent.role_template,
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
            "SELECT id, name, role_template, provider_kind, provider_name, model, created_at
             FROM agents ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                role_template: row.get(2)?,
                provider_kind: row.get(3)?,
                provider_name: row.get(4)?,
                model: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_agent(&self, id: &str) -> rusqlite::Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, role_template, provider_kind, provider_name, model, created_at
             FROM agents WHERE id = ?1",
            params![id],
            |row| {
                Ok(Agent {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    role_template: row.get(2)?,
                    provider_kind: row.get(3)?,
                    provider_name: row.get(4)?,
                    model: row.get(5)?,
                    created_at: row.get(6)?,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list_agent() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage
            .create_agent("Full-Stack Developer", Some("Full-Stack Developer"), "cloud", "anthropic", "claude")
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
            .create_agent("Local Agent", None, "local", "ollama", "llama3.1:8b")
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
        let agent = storage.create_agent("Test", None, "cloud", "anthropic", "claude").unwrap();
        assert!(storage.list_file_access_grants(&agent.id).unwrap().is_empty());

        let grant = storage.grant_folder_access(&agent.id, "/tmp/notes").unwrap();
        let grants = storage.list_file_access_grants(&agent.id).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].folder_path, "/tmp/notes");

        storage.revoke_file_access_grant(&grant.id).unwrap();
        assert!(storage.list_file_access_grants(&agent.id).unwrap().is_empty());
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
