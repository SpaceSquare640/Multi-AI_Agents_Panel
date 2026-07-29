//! Tauri commands backing the AI Control Center page: API Key management,
//! cloud model pickers, and local Ollama model management.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::agent_manager::curated_models::{self, CuratedModel};
use crate::agent_manager::providers::{ollama, ChatMessage};
use crate::agent_manager::role_templates::{self, RoleTemplate};
use crate::agent_manager::{self};
use crate::file_access;
use crate::key_vault;
use crate::skill_manager::{self, SkillManifest};
use crate::storage::{Agent, FileAccessGrant, Message, ProviderKey, Session, SkillAccessGrant, Storage, UsageSummary};
use crate::{SkillRuntimeState, SkillsDir};

/// A `ProviderKey` plus a masked preview of the secret (never the real
/// value) — this is what the frontend actually renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyView {
    #[serde(flatten)]
    pub meta: ProviderKey,
    /// e.g. "sk-or-v1-••••7290", or "(missing)" if the vault entry vanished.
    pub masked_secret: String,
}

fn mask(secret: &str) -> String {
    let tail: String = secret.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("••••{tail}")
}

fn to_view(meta: ProviderKey) -> ProviderKeyView {
    let masked_secret = key_vault::get_secret(&meta.id)
        .ok()
        .flatten()
        .map(|s| mask(&s))
        .unwrap_or_else(|| "(missing)".to_string());
    ProviderKeyView { meta, masked_secret }
}

#[tauri::command]
pub fn list_provider_keys(storage: State<Storage>) -> Result<Vec<ProviderKeyView>, String> {
    storage
        .list_provider_keys()
        .map(|keys| keys.into_iter().map(to_view).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_provider_key(
    storage: State<Storage>,
    provider: String,
    secret: String,
    label: Option<String>,
    model_hint: Option<String>,
) -> Result<ProviderKeyView, String> {
    let meta = storage
        .create_provider_key(&provider, label.as_deref(), model_hint.as_deref())
        .map_err(|e| e.to_string())?;
    key_vault::set_secret(&meta.id, &secret).map_err(|e| e.to_string())?;
    Ok(to_view(meta))
}

/// One line of a batch import: same shape as `add_provider_key`'s inputs.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchKeyEntry {
    pub provider: String,
    pub secret: String,
    pub label: Option<String>,
    pub model_hint: Option<String>,
}

#[tauri::command]
pub fn batch_add_provider_keys(
    storage: State<Storage>,
    entries: Vec<BatchKeyEntry>,
) -> Result<Vec<ProviderKeyView>, String> {
    entries
        .into_iter()
        .map(|entry| {
            let meta = storage
                .create_provider_key(&entry.provider, entry.label.as_deref(), entry.model_hint.as_deref())
                .map_err(|e| e.to_string())?;
            key_vault::set_secret(&meta.id, &entry.secret).map_err(|e| e.to_string())?;
            Ok(to_view(meta))
        })
        .collect()
}

#[tauri::command]
pub fn delete_provider_key(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.delete_provider_key(&id).map_err(|e| e.to_string())?;
    key_vault::delete_secret(&id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_usage_summary(storage: State<Storage>) -> Result<Vec<UsageSummary>, String> {
    storage.usage_summary().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_curated_models(provider: String) -> Result<Vec<CuratedModel>, String> {
    match provider.as_str() {
        "anthropic" => Ok(curated_models::anthropic_models()),
        "openrouter" => Ok(curated_models::openrouter_models()),
        "ollama" => Ok(curated_models::ollama_models()),
        other => Err(format!("no curated model list for '{other}' yet")),
    }
}

#[tauri::command]
pub fn ollama_is_running() -> bool {
    ollama::is_running()
}

#[tauri::command]
pub fn list_ollama_installed_models() -> Result<Vec<ollama::OllamaModel>, String> {
    ollama::list_installed().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pull_ollama_model(name: String) -> Result<(), String> {
    ollama::pull_model(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_ollama_model(name: String) -> Result<(), String> {
    ollama::delete_model(&name).map_err(|e| e.to_string())
}

// --- Independent Session chat (agents, sessions, messages) ---

#[tauri::command]
pub fn list_agents(storage: State<Storage>) -> Result<Vec<Agent>, String> {
    storage.list_agents().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_agent(
    storage: State<Storage>,
    name: String,
    role_template: Option<String>,
    system_prompt: Option<String>,
    provider_kind: String,
    provider_name: String,
    model: String,
) -> Result<Agent, String> {
    storage
        .create_agent(
            &name,
            role_template.as_deref(),
            system_prompt.as_deref(),
            &provider_kind,
            &provider_name,
            &model,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(storage: State<Storage>) -> Result<Vec<Session>, String> {
    storage.list_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_independent_session(
    storage: State<Storage>,
    title: String,
    agent_id: String,
) -> Result<Session, String> {
    let session = storage.create_session("independent", &title).map_err(|e| e.to_string())?;
    storage
        .add_agent_to_session(&session.id, &agent_id)
        .map_err(|e| e.to_string())?;
    Ok(session)
}

#[tauri::command]
pub fn list_messages(storage: State<Storage>, session_id: String) -> Result<Vec<Message>, String> {
    storage.list_messages(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session_agent_id(storage: State<Storage>, session_id: String) -> Result<Option<String>, String> {
    let agents = storage.agents_for_session(&session_id).map_err(|e| e.to_string())?;
    Ok(agents.into_iter().next())
}

/// Expands `@file:<path>` references in a chat message into the referenced
/// file's content, subject to the same File Access authorization as
/// everything else — an unauthorized or missing reference fails the whole
/// send rather than silently sending a broken reference to the model.
/// Only ever applied to the message about to be sent, never to history,
/// so old turns don't re-read files (or fail on ones since deleted/moved)
/// every time the conversation continues.
fn expand_file_references(storage: &Storage, agent_id: &str, content: &str) -> Result<String, String> {
    let mut expanded = content.to_string();
    for token in content.split_whitespace() {
        if let Some(path) = token.strip_prefix("@file:") {
            let file_content = file_access::read_file(storage, agent_id, std::path::Path::new(path))
                .map_err(|e| e.to_string())?;
            expanded.push_str(&format!("\n\n[Contents of {path}]\n{file_content}"));
        }
    }
    Ok(expanded)
}

/// Sends a user message in a session, gets the (single, for an independent
/// session) agent's reply, and persists both. Returns the assistant's
/// message. The user's message is persisted even if the agent call then
/// fails, so a retry doesn't lose it.
#[tauri::command]
pub fn send_chat_message(storage: State<Storage>, session_id: String, content: String) -> Result<Message, String> {
    storage
        .add_message(&session_id, None, "user", &content)
        .map_err(|e| e.to_string())?;

    let agent_id = storage
        .agents_for_session(&session_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| format!("session {session_id} has no agent"))?;
    let agent = storage
        .get_agent(&agent_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent {agent_id} not found"))?;

    let expanded_content = expand_file_references(&storage, &agent_id, &content)?;

    let mut history: Vec<ChatMessage> = storage
        .list_messages(&session_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    if let Some(last) = history.last_mut() {
        last.content = expanded_content;
    }
    if let Some(system_prompt) = &agent.system_prompt {
        history.insert(
            0,
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            },
        );
    }

    let reply = agent_manager::send_message(&storage, &agent, &history).map_err(|e| e.to_string())?;

    storage
        .add_message(&session_id, Some(&agent_id), "assistant", &reply)
        .map_err(|e| e.to_string())
}

// --- File Access grants ---
//
// Note what's intentionally missing from this command surface: there's no
// `request_folder_access` that opens the OS folder picker itself. The
// frontend calls `@tauri-apps/plugin-dialog`'s picker directly and only
// invokes `grant_folder_access` with whatever the user actually chose —
// that keeps the one place capable of creating a grant tied to a real,
// user-initiated OS dialog, not a Rust-side prompt that could be called
// from anywhere.

#[tauri::command]
pub fn grant_folder_access(
    storage: State<Storage>,
    agent_id: String,
    folder_path: String,
) -> Result<FileAccessGrant, String> {
    storage.grant_folder_access(&agent_id, &folder_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_file_access_grants(storage: State<Storage>, agent_id: String) -> Result<Vec<FileAccessGrant>, String> {
    storage.list_file_access_grants(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revoke_file_access_grant(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.revoke_file_access_grant(&id).map_err(|e| e.to_string())
}

// --- Role Templates ("1 人公司") ---

#[tauri::command]
pub fn list_default_role_templates() -> Vec<RoleTemplate> {
    role_templates::default_templates()
}

#[tauri::command]
pub fn list_custom_role_templates(storage: State<Storage>) -> Result<Vec<RoleTemplate>, String> {
    storage
        .list_custom_role_templates()
        .map(|templates| templates.into_iter().map(RoleTemplate::from).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_custom_role_template(
    storage: State<Storage>,
    name: String,
    description: String,
    system_prompt: String,
    suggested_provider_kind: Option<String>,
    suggested_provider_name: Option<String>,
    suggested_model: Option<String>,
) -> Result<RoleTemplate, String> {
    storage
        .create_custom_role_template(
            &name,
            &description,
            &system_prompt,
            suggested_provider_kind.as_deref(),
            suggested_provider_name.as_deref(),
            suggested_model.as_deref(),
        )
        .map(RoleTemplate::from)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_custom_role_template(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.delete_custom_role_template(&id).map_err(|e| e.to_string())
}

// --- Skills (Python bridge) ---

#[tauri::command]
pub fn list_skills(skills_dir: State<SkillsDir>) -> Vec<SkillManifest> {
    skill_manager::discover_skills(&skills_dir.0)
}

#[tauri::command]
pub fn grant_skill_access(
    storage: State<Storage>,
    agent_id: String,
    skill_name: String,
) -> Result<SkillAccessGrant, String> {
    storage.grant_skill_access(&agent_id, &skill_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_skill_access_grants(storage: State<Storage>, agent_id: String) -> Result<Vec<SkillAccessGrant>, String> {
    storage.list_skill_access_grants(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revoke_skill_access(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.revoke_skill_access_grant(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn invoke_skill(
    storage: State<Storage>,
    runtime: State<SkillRuntimeState>,
    agent_id: String,
    skill_name: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let guard = runtime.0.lock().unwrap();
    skill_manager::invoke_skill(&storage, guard.as_ref(), &agent_id, &skill_name, payload).map_err(|e| e.to_string())
}
