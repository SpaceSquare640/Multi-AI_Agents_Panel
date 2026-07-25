//! Storage: local-only SQLite persistence for conversations, agent
//! settings, and task-graph run logs. No cloud storage of any kind.
//! Design: `Multi-AI Agent Panel Document/01 Project Overview/Vision & Goals.md`

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct Session {
    pub id: String,
    /// "independent" | "group"
    pub kind: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    }

    #[test]
    fn session_and_messages_round_trip() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage
            .create_agent("Local Agent", None, "local", "ollama", "llama3.1:8b")
            .unwrap();
        let session = storage.create_session("independent", "Test session").unwrap();
        storage.add_agent_to_session(&session.id, &agent.id).unwrap();

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
}
