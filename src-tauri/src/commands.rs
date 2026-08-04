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
use crate::orchestrator::{self, GroupChatError};
use crate::session_manager;
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
        "openai" => Ok(curated_models::openai_models()),
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

/// Pins (or, with `providerKeyId: null`, un-pins) which Key Vault entry an
/// agent uses for cloud calls — see `Agent::pinned_provider_key_id`.
/// Separate from `create_agent` so that command's signature doesn't grow
/// for an optional, post-creation choice.
#[tauri::command]
pub fn pin_agent_provider_key(
    storage: State<Storage>,
    agent_id: String,
    provider_key_id: Option<String>,
) -> Result<(), String> {
    storage.pin_agent_provider_key(&agent_id, provider_key_id.as_deref()).map_err(|e| e.to_string())
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

// --- Group Chat ---
//
// Independent Session vs. Group Chat share the same `sessions` /
// `session_agents` / `messages` tables (see `storage`) — a Group Chat is
// just a session with `kind = "group"` and more than one member. What's
// different is turn-taking (`session_manager`) and the loop safety-net /
// meeting-end summarization (`orchestrator`).

#[tauri::command]
pub fn create_group_session(
    storage: State<Storage>,
    title: String,
    agent_ids: Vec<String>,
) -> Result<Session, String> {
    if agent_ids.is_empty() {
        return Err("a Group Chat needs at least one participating agent".to_string());
    }
    let session = storage.create_session("group", &title).map_err(|e| e.to_string())?;
    for agent_id in &agent_ids {
        storage.add_agent_to_session(&session.id, agent_id).map_err(|e| e.to_string())?;
    }
    Ok(session)
}

fn session_members(storage: &Storage, session_id: &str) -> Result<Vec<Agent>, String> {
    let ids = storage.agents_for_session(session_id).map_err(|e| e.to_string())?;
    ids.into_iter()
        .filter_map(|id| storage.get_agent(&id).transpose())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_session_members(storage: State<Storage>, session_id: String) -> Result<Vec<Agent>, String> {
    session_members(&storage, &session_id)
}

/// Builds the message history a given member sees, framing every *other*
/// member's turns as `role: "user"` content prefixed with their name —
/// the only way to make one-user/one-assistant provider APIs represent a
/// multi-party conversation without the model mistaking someone else's
/// words for its own. The speaking agent's own past turns stay
/// `role: "assistant"` so it recognizes its own voice.
fn build_group_history_for_speaker(
    storage: &Storage,
    session_id: &str,
    speaking_agent_id: &str,
) -> Result<Vec<ChatMessage>, String> {
    let names: std::collections::HashMap<String, String> = session_members(storage, session_id)?
        .into_iter()
        .map(|agent| (agent.id, agent.name))
        .collect();

    let history = storage.list_messages(session_id).map_err(|e| e.to_string())?;
    Ok(history
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| match (&m.role[..], &m.agent_id) {
            ("assistant", Some(agent_id)) if agent_id == speaking_agent_id => {
                ChatMessage { role: "assistant".to_string(), content: m.content }
            }
            ("assistant", Some(agent_id)) => {
                let name = names.get(agent_id).cloned().unwrap_or_else(|| "another agent".to_string());
                ChatMessage { role: "user".to_string(), content: format!("[{name}]: {}", m.content) }
            }
            _ => ChatMessage { role: "user".to_string(), content: m.content },
        })
        .collect())
}

/// Runs exactly one agent turn: resolves the speaker (mention or
/// rotation), builds their view of the conversation, calls them, persists
/// the reply, and saves the updated turn-taking state. Shared by
/// `send_group_message` (after the user's own message) and
/// `advance_group_turn` (no new user message — "let them keep talking").
fn run_one_group_turn(storage: &Storage, session_id: &str, mention: Option<&str>) -> Result<Message, String> {
    let member_ids = storage.agents_for_session(session_id).map_err(|e| e.to_string())?;
    let state = storage.get_group_session_state(session_id).map_err(|e| e.to_string())?;

    let (speaker_id, new_cursor) =
        orchestrator::plan_next_turn(&state, &member_ids, mention).map_err(|e| e.to_string())?;

    let agent = storage
        .get_agent(&speaker_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent {speaker_id} not found"))?;

    let mut history = build_group_history_for_speaker(storage, session_id, &speaker_id)?;
    if let Some(system_prompt) = &agent.system_prompt {
        history.insert(0, ChatMessage { role: "system".to_string(), content: system_prompt.clone() });
    }

    let reply = agent_manager::send_message(storage, &agent, &history).map_err(|e| e.to_string())?;

    let saved = storage
        .add_message(session_id, Some(&speaker_id), "assistant", &reply)
        .map_err(|e| e.to_string())?;

    storage
        .save_group_session_state(&crate::storage::GroupSessionState {
            session_id: session_id.to_string(),
            rotation_cursor: new_cursor as i64,
            consecutive_agent_turns: state.consecutive_agent_turns + 1,
        })
        .map_err(|e| e.to_string())?;

    Ok(saved)
}

/// A user message in a Group Chat: persists it, resets the loop
/// safety-net (a real user spoke), then runs exactly one agent turn —
/// the `@mentioned` agent if any, otherwise whoever is next in rotation.
#[tauri::command]
pub fn send_group_message(storage: State<Storage>, session_id: String, content: String) -> Result<Message, String> {
    storage.add_message(&session_id, None, "user", &content).map_err(|e| e.to_string())?;
    storage.reset_group_session_turn_counter(&session_id).map_err(|e| e.to_string())?;

    let members = session_members(&storage, &session_id)?;
    let mention = session_manager::parse_mention(&content, &members);

    run_one_group_turn(&storage, &session_id, mention.as_deref())
}

/// Lets the meeting continue without new user input — one more agent
/// turn in rotation. This is the path the E6001 loop safety-net actually
/// guards, since nothing else prevents calling this repeatedly.
#[tauri::command]
pub fn advance_group_turn(storage: State<Storage>, session_id: String) -> Result<Message, String> {
    run_one_group_turn(&storage, &session_id, None)
}

/// Ends the meeting: picks a summarizer (explicit choice, else a Product
/// Lead member, else whoever joined first — `orchestrator::pick_summarizer`),
/// asks them to summarize the discussion so far, and persists that as the
/// final message. Per `Session Types.md`, deciding whether to fold this
/// summary into any member's long-term memory is a separate, explicit,
/// opt-in action — and there is no long-term memory store to write into
/// yet, so that step isn't implemented (see Backlog).
#[tauri::command]
pub fn end_group_chat_meeting(
    storage: State<Storage>,
    session_id: String,
    summarizer_agent_id: Option<String>,
) -> Result<Message, String> {
    let members = session_members(&storage, &session_id)?;
    let summarizer = orchestrator::pick_summarizer(&members, summarizer_agent_id.as_deref())
        .cloned()
        .ok_or_else(|| GroupChatError::NoMembers.to_string())?;

    let mut history = build_group_history_for_speaker(&storage, &session_id, &summarizer.id)?;
    history.push(ChatMessage {
        role: "user".to_string(),
        content: "Please summarize this meeting: the key points discussed, any decisions reached, and any open questions left for the user.".to_string(),
    });
    if let Some(system_prompt) = &summarizer.system_prompt {
        history.insert(0, ChatMessage { role: "system".to_string(), content: system_prompt.clone() });
    }

    let summary = agent_manager::send_message(&storage, &summarizer, &history).map_err(|e| e.to_string())?;

    storage
        .add_message(&session_id, Some(&summarizer.id), "assistant", &summary)
        .map_err(|e| e.to_string())
}

/// Copies everything a given member has seen in this Group Chat into a
/// brand-new Independent Session, per `Session Types.md`'s "拉出成獨立
/// Session" rule. The new session is a one-time snapshot — it does not
/// stay in sync with the Group Chat afterward, matching the decided
/// "each side stays independent" memory-isolation rule.
#[tauri::command]
pub fn pull_out_to_independent_session(
    storage: State<Storage>,
    session_id: String,
    agent_id: String,
    title: String,
) -> Result<Session, String> {
    let history = build_group_history_for_speaker(&storage, &session_id, &agent_id)?;

    let new_session = storage.create_session("independent", &title).map_err(|e| e.to_string())?;
    storage.add_agent_to_session(&new_session.id, &agent_id).map_err(|e| e.to_string())?;

    for message in history {
        storage
            .add_message(
                &new_session.id,
                if message.role == "assistant" { Some(&agent_id) } else { None },
                &message.role,
                &message.content,
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(new_session)
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

/// Runs a Skill on `agent_id`'s behalf and persists the result into
/// `session_id`'s message history (`role: "system"`) so it shows up in
/// the transcript the same way a file attachment or a group-chat
/// cross-agent message does — visible, but not attributed to "user" or
/// "assistant". Goes through the exact same `skill_manager::invoke_skill`
/// gate (Guardrails injection screen, per-agent allowlist) as
/// `invoke_skill` above; this only adds "and remember it happened."
#[tauri::command]
pub fn run_skill_in_session(
    storage: State<Storage>,
    runtime: State<SkillRuntimeState>,
    session_id: String,
    agent_id: String,
    skill_name: String,
    payload: serde_json::Value,
) -> Result<Message, String> {
    let result = {
        let guard = runtime.0.lock().unwrap();
        skill_manager::invoke_skill(&storage, guard.as_ref(), &agent_id, &skill_name, payload).map_err(|e| e.to_string())?
    };
    let content = format!(
        "[Skill \"{skill_name}\" result]\n{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
    );
    storage.add_message(&session_id, Some(&agent_id), "system", &content).map_err(|e| e.to_string())
}

/// Live end-to-end tests exercising the internal (`&Storage`-only, no
/// `tauri::State`) Group Chat functions against real OpenRouter agents.
/// Not run in CI (needs a real free API key) — run manually with
/// `cargo test commands::live -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;
    use crate::key_vault;

    fn make_openrouter_agent(storage: &Storage, name: &str, system_prompt: &str, api_key: &str) -> Agent {
        let meta = storage.create_provider_key("openrouter", Some(name), None).unwrap();
        key_vault::set_secret(&meta.id, api_key).unwrap();
        storage
            .create_agent(name, None, Some(system_prompt), "cloud", "openrouter", "inclusionai/ling-3.0-flash:free")
            .unwrap()
    }

    #[test]
    #[ignore]
    fn a_real_group_chat_round_robins_between_two_real_agents() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY").expect("set OPENROUTER_TEST_KEY to run this test");
        let storage = Storage::open_in_memory().unwrap();

        let alice = make_openrouter_agent(
            &storage,
            "Alice",
            "You are Alice. No matter what is asked or said, reply with exactly one word: ALICE.",
            &api_key,
        );
        let bob = make_openrouter_agent(
            &storage,
            "Bob",
            "You are Bob. No matter what is asked or said, reply with exactly one word: BOB.",
            &api_key,
        );

        let session = storage.create_session("group", "Live round robin test").unwrap();
        storage.add_agent_to_session(&session.id, &alice.id).unwrap();
        storage.add_agent_to_session(&session.id, &bob.id).unwrap();

        storage.add_message(&session.id, None, "user", "Let's begin the meeting.").unwrap();
        storage.reset_group_session_turn_counter(&session.id).unwrap();

        // No @mention: round-robin should pick the first-joined member (Alice) first.
        let first_reply = run_one_group_turn(&storage, &session.id, None).unwrap();
        assert_eq!(first_reply.agent_id.as_deref(), Some(alice.id.as_str()));
        assert!(first_reply.content.to_uppercase().contains("ALICE"), "got: {}", first_reply.content);

        // Then Bob, in rotation order.
        let second_reply = run_one_group_turn(&storage, &session.id, None).unwrap();
        assert_eq!(second_reply.agent_id.as_deref(), Some(bob.id.as_str()));
        assert!(second_reply.content.to_uppercase().contains("BOB"), "got: {}", second_reply.content);

        // An @mention pulls Alice back in out of turn.
        let mentioned_reply = run_one_group_turn(&storage, &session.id, Some(&alice.id)).unwrap();
        assert_eq!(mentioned_reply.agent_id.as_deref(), Some(alice.id.as_str()));

        // Rotation resumes where it left off: Alice (index 0) was next before
        // the mention, so it should still be Alice's turn now.
        let fourth_reply = run_one_group_turn(&storage, &session.id, None).unwrap();
        assert_eq!(fourth_reply.agent_id.as_deref(), Some(alice.id.as_str()));
    }

    #[test]
    #[ignore]
    fn ending_a_meeting_produces_a_real_summary_from_the_product_lead() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY").expect("set OPENROUTER_TEST_KEY to run this test");
        let storage = Storage::open_in_memory().unwrap();

        let lead_meta = storage.create_provider_key("openrouter", Some("lead"), None).unwrap();
        key_vault::set_secret(&lead_meta.id, &api_key).unwrap();
        let lead = storage
            .create_agent(
                "Lead",
                Some("Product Lead"),
                Some("You are the Product Lead. When asked to summarize, reply with exactly: SUMMARY DONE."),
                "cloud",
                "openrouter",
                "inclusionai/ling-3.0-flash:free",
            )
            .unwrap();
        let dev = make_openrouter_agent(
            &storage,
            "Dev",
            "You are a developer. No matter what is asked, reply with exactly one word: DEV.",
            &api_key,
        );

        let session = storage.create_session("group", "Live summary test").unwrap();
        // Dev joins first, Lead second — proves summarizer selection prefers
        // the Product Lead role over "first to join".
        storage.add_agent_to_session(&session.id, &dev.id).unwrap();
        storage.add_agent_to_session(&session.id, &lead.id).unwrap();
        storage.add_message(&session.id, None, "user", "What should we build first?").unwrap();

        let members = session_members(&storage, &session.id).unwrap();
        let summarizer = orchestrator::pick_summarizer(&members, None).unwrap().clone();
        assert_eq!(summarizer.id, lead.id, "Product Lead should be picked over the first-joined member");

        let mut history = build_group_history_for_speaker(&storage, &session.id, &summarizer.id).unwrap();
        history.push(ChatMessage { role: "user".to_string(), content: "Please summarize this meeting.".to_string() });
        history.insert(0, ChatMessage { role: "system".to_string(), content: summarizer.system_prompt.clone().unwrap() });

        let summary = agent_manager::send_message(&storage, &summarizer, &history).unwrap();
        assert!(summary.to_uppercase().contains("SUMMARY DONE"), "got: {summary}");
    }

    #[test]
    #[ignore]
    fn running_a_skill_in_a_session_persists_its_result_as_a_system_message() {
        // Exercises the same two calls `run_skill_in_session` makes, minus
        // the `tauri::State` wrapper (which can't be constructed outside a
        // running app) — spawns the real Python bridge, same as
        // `skill_manager::live`.
        let skills_dir = skill_manager::resolve_skills_dir(None);
        let runtime = skill_manager::SkillRuntime::start(&skills_dir)
            .expect("bridge should start with a real Python on PATH");

        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        storage.grant_skill_access(&agent.id, "example_skill").unwrap();
        let session = storage.create_session("independent", "Skill test").unwrap();
        storage.add_agent_to_session(&session.id, &agent.id).unwrap();

        let result = skill_manager::invoke_skill(
            &storage,
            Some(&runtime),
            &agent.id,
            "example_skill",
            serde_json::json!({"ping": "pong"}),
        )
        .unwrap();
        let content = format!(
            "[Skill \"example_skill\" result]\n{}",
            serde_json::to_string_pretty(&result).unwrap()
        );
        let saved = storage.add_message(&session.id, Some(&agent.id), "system", &content).unwrap();

        assert_eq!(saved.role, "system");
        assert!(saved.content.contains("ping"));
        let messages = storage.list_messages(&session.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, saved.id);
    }
}
