//! Tauri commands backing the AI Control Center page: API Key management,
//! cloud model pickers, and local Ollama model management.

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::agent_manager::curated_models::{self, CuratedModel};
use crate::agent_manager::openrouter_catalog::{self, OpenRouterCatalogState, OpenRouterModelsResult};
use crate::agent_manager::providers::{ollama, ChatMessage};
use crate::game_agent::{self, GameAgentState, RecordingState};
use crate::guardrails;
use crate::agent_manager::role_templates::{self, RoleTemplate};
use crate::agent_manager::{self};
use crate::file_access;
use crate::key_vault;
use crate::mcp_manager::{self, McpTool};
use crate::ml_engine::{self, MlCapabilityManifest};
use crate::orchestrator::{self, GroupChatError};
use crate::session_manager;
use crate::skill_manager::{self, SkillManifest};
use crate::update_check;
use crate::storage::{
    Agent, AgentFallbackProvider, FileAccessGrant, McpAccessGrant, McpServer, MlAccessGrant, Message, ProviderKey,
    Session, SkillAccessGrant, Storage, UsageSummary,
};
use crate::{MlDir, MlEngineRuntimeState, SkillDirs, SkillRuntimeState};

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

/// Batch-import keys from files picked via the OS file picker: filename
/// (without extension) becomes the label, file content (trimmed) becomes
/// the secret. Same `provider` is applied to every file in the batch.
#[tauri::command]
pub fn import_provider_keys_from_files(
    storage: State<Storage>,
    provider: String,
    paths: Vec<String>,
) -> Result<Vec<ProviderKeyView>, String> {
    paths
        .into_iter()
        .map(|path| {
            let secret = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {path}: {e}"))?
                .trim()
                .to_string();
            if secret.is_empty() {
                return Err(format!("{path} is empty"));
            }
            let label = std::path::Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            let meta = storage
                .create_provider_key(&provider, label.as_deref(), None)
                .map_err(|e| e.to_string())?;
            key_vault::set_secret(&meta.id, &secret).map_err(|e| e.to_string())?;
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
        "colibri" => Ok(curated_models::colibri_models()),
        "omniroute" => Ok(curated_models::omniroute_models()),
        other => Err(format!("no curated model list for '{other}' yet")),
    }
}

/// Live OpenRouter model catalog (see `agent_manager::openrouter_catalog`
/// for the caching/fallback policy) — separate from `list_curated_models`,
/// which stays purely static for the other providers per the design
/// doc's OpenRouter-specific decision.
#[tauri::command]
pub fn list_openrouter_models_live(
    cache: State<OpenRouterCatalogState>,
    force_refresh: bool,
) -> OpenRouterModelsResult {
    openrouter_catalog::list_models(&cache, force_refresh)
}

/// Starts the Track A game-playing vision loop (see `game_agent` module
/// docs) — real mouse/keyboard automation on the user's machine, only
/// ever started by this explicit, user-triggered command.
#[tauri::command]
pub fn start_game_agent(state: State<GameAgentState>, model: String, prompt: String) -> Result<(), String> {
    game_agent::start(&state, model, prompt)
}

#[tauri::command]
pub fn stop_game_agent(state: State<GameAgentState>) {
    game_agent::stop(&state);
}

#[tauri::command]
pub fn game_agent_status(state: State<GameAgentState>) -> bool {
    game_agent::is_running(&state)
}

/// Starts Track B's `record` stage (see `game_agent`'s recording
/// section) as a background Python subprocess — a human demonstration
/// session, not the Track A vision-agent loop.
#[tauri::command]
pub fn start_recording_session(
    state: State<RecordingState>,
    recording_dir: State<crate::RecordingDir>,
    session: String,
    output_dir: String,
) -> Result<(), String> {
    game_agent::start_recording(&state, &recording_dir.0, &session, &output_dir)
}

#[tauri::command]
pub fn stop_recording_session(state: State<RecordingState>) -> Result<(), String> {
    game_agent::stop_recording(&state)
}

#[tauri::command]
pub fn recording_status(state: State<RecordingState>) -> bool {
    game_agent::is_recording(&state)
}

#[tauri::command]
pub fn ollama_is_running() -> bool {
    ollama::is_running()
}

#[tauri::command]
pub fn list_ollama_installed_models() -> Result<Vec<ollama::OllamaModel>, String> {
    ollama::list_installed().map_err(|e| e.to_string())
}

/// Pulls (installs) an Ollama model, emitting an `ollama-pull-progress`
/// event (`{ name, status, completed, total, percent }`) for every line
/// Ollama streams back — real progress, not a blocking call with only a
/// static "loading" indicator on the frontend.
#[tauri::command]
pub fn pull_ollama_model(app: tauri::AppHandle, name: String) -> Result<(), String> {
    use tauri::Emitter;
    ollama::pull_model_with_progress(&name, |progress| {
        let _ = app.emit(
            "ollama-pull-progress",
            serde_json::json!({
                "name": name,
                "status": progress.status,
                "completed": progress.completed,
                "total": progress.total,
                "percent": progress.percent,
            }),
        );
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_ollama_model(name: String) -> Result<(), String> {
    ollama::delete_model(&name).map_err(|e| e.to_string())
}

/// Reads `OLLAMA_MODELS` from *this app's own process environment* —
/// best-effort information only. Ollama is an external, independently
/// launched service (this app only calls its `localhost:11434` API, see
/// `agent_manager/providers/ollama.rs`); if Ollama runs as a background
/// service or was started from a different shell/session, its actual
/// environment can differ from this app's, so a `None` here does not
/// necessarily mean Ollama itself has no override configured. Purely a
/// convenience hint for the AI Control Center's guidance panel — see
/// [[Unified Data Folder & Custom Skills Design]] for why this app does
/// not attempt to control or redirect Ollama's model storage itself.
#[tauri::command]
pub fn ollama_models_env_hint() -> Option<String> {
    std::env::var("OLLAMA_MODELS").ok()
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

/// Adds one step to an agent's cross-provider fallback chain — tried in
/// the order added, only after the agent's own primary provider has
/// exhausted its own key rotation. See `agent_manager::dispatch`.
#[tauri::command]
pub fn add_agent_fallback_provider(
    storage: State<Storage>,
    agent_id: String,
    provider_kind: String,
    provider_name: String,
    model: String,
) -> Result<AgentFallbackProvider, String> {
    storage
        .add_agent_fallback_provider(&agent_id, &provider_kind, &provider_name, &model)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_agent_fallback_providers(storage: State<Storage>, agent_id: String) -> Result<Vec<AgentFallbackProvider>, String> {
    storage.list_agent_fallback_providers(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_agent_fallback_provider(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.remove_agent_fallback_provider(&id).map_err(|e| e.to_string())
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

/// Same shape as `send_chat_message`, but runs the agent with tool
/// calling enabled (see `agent_manager::function_calling` — Anthropic
/// agents only, others return an error). The tools offered to the model
/// are exactly this agent's *granted* skills (`list_skill_access_grants`),
/// filtered against the discovered manifests — same allowlist a human
/// would see, not an expanded set. Any tool call the model makes still
/// goes through `skill_manager::invoke_skill`'s Guardrails-then-allowlist
/// gate. Each tool call the model actually executes is also persisted as
/// its own `role: "system"` message (mirrors `run_skill_in_session`), so
/// the session transcript shows what the model did, not just its final
/// reply.
#[tauri::command]
pub fn send_chat_message_with_tools(
    storage: State<Storage>,
    skill_dirs: State<SkillDirs>,
    skill_runtime: State<SkillRuntimeState>,
    session_id: String,
    content: String,
) -> Result<Message, String> {
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

    let mut manifests = skill_manager::discover_skills_tagged(&skill_dirs.builtin, "built-in");
    manifests.extend(skill_manager::discover_skills_tagged(&skill_dirs.custom, "custom"));
    let granted: std::collections::HashSet<String> = storage
        .list_skill_access_grants(&agent_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|g| g.skill_name)
        .collect();
    let available_skills: Vec<SkillManifest> = manifests.into_iter().filter(|m| granted.contains(&m.name)).collect();

    let expanded_content = expand_file_references(&storage, &agent_id, &content)?;

    let guard = skill_runtime.0.lock().unwrap();
    let result = agent_manager::function_calling::run(&storage, guard.as_ref(), &agent, &available_skills, &expanded_content)
        .map_err(|e| e.to_string())?;

    for call in &result.tool_calls {
        let content = format!(
            "[Tool call \"{}\" invoked by the model]\nInput: {}\nOutput: {}",
            call.tool_name,
            call.input,
            call.output
        );
        storage.add_message(&session_id, Some(&agent_id), "system", &content).map_err(|e| e.to_string())?;
    }

    storage
        .add_message(&session_id, Some(&agent_id), "assistant", &result.reply)
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

/// What `run_one_group_turn` (and therefore `send_group_message`/
/// `advance_group_turn`) can produce: either a completed turn, or a
/// pause for the local→cloud boundary confirmation decided in
/// `Orchestration Design.md` (Error Code Registry E6004) — not
/// persisted as a failure, since the rule working as designed isn't an
/// error, the same reasoning `guardrails`' E9003 uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GroupTurnResult {
    Message(Message),
    /// The next speaker would be a cloud Agent, and their view of the
    /// conversation includes content a local Agent produced, and this
    /// session hasn't confirmed sending local content to the cloud yet.
    /// `previewContent` is exactly what would be sent — the frontend
    /// shows it, then either calls `confirm_local_to_cloud_boundary`
    /// and retries, or the user cancels and nothing happens.
    BoundaryConfirmationNeeded { error_code: &'static str, preview_content: String },
}

/// The text of every local-Agent-authored message currently in
/// `session_id`'s history — exactly what `run_one_group_turn` would be
/// about to send to a cloud Agent, used both to decide whether a
/// confirmation is needed and as the content shown in that confirmation.
fn local_agent_content_preview(storage: &Storage, session_id: &str) -> Result<Vec<String>, String> {
    let members = session_members(storage, session_id)?;
    let local_agent_ids: std::collections::HashSet<String> =
        members.iter().filter(|a| a.provider_kind == "local").map(|a| a.id.clone()).collect();
    if local_agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let history = storage.list_messages(session_id).map_err(|e| e.to_string())?;
    Ok(history
        .into_iter()
        .filter(|m| m.role == "assistant" && m.agent_id.as_deref().is_some_and(|id| local_agent_ids.contains(id)))
        .map(|m| m.content)
        .collect())
}

/// Explicit, per-session opt-in for sending local-Agent content across
/// the local→cloud boundary — see `Orchestration Design.md`'s decided
/// "本場 Group Chat 都記住我的選擇" rule.
#[tauri::command]
pub fn confirm_local_to_cloud_boundary(storage: State<Storage>, session_id: String) -> Result<(), String> {
    storage.grant_local_to_cloud_consent(&session_id).map_err(|e| e.to_string())
}

/// Runs exactly one agent turn: resolves the speaker (mention or
/// rotation), builds their view of the conversation, calls them, persists
/// the reply, and saves the updated turn-taking state. Shared by
/// `send_group_message` (after the user's own message) and
/// `advance_group_turn` (no new user message — "let them keep talking").
///
/// Before ever calling a cloud provider, checks the local→cloud boundary
/// rule (E6004): if the resolved speaker is a cloud Agent, their history
/// contains content a local Agent produced, and this session hasn't
/// confirmed sending local content to the cloud yet, this returns
/// `BoundaryConfirmationNeeded` instead of calling the provider — no
/// message is persisted, no turn-taking state changes, so retrying after
/// confirmation resolves the exact same speaker.
fn run_one_group_turn(storage: &Storage, session_id: &str, mention: Option<&str>) -> Result<GroupTurnResult, String> {
    let member_ids = storage.agents_for_session(session_id).map_err(|e| e.to_string())?;
    let state = storage.get_group_session_state(session_id).map_err(|e| e.to_string())?;

    let (speaker_id, new_cursor) =
        orchestrator::plan_next_turn(&state, &member_ids, mention).map_err(|e| e.to_string())?;

    let agent = storage
        .get_agent(&speaker_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent {speaker_id} not found"))?;

    if agent.provider_kind == "cloud" && !storage.has_local_to_cloud_consent(session_id).map_err(|e| e.to_string())? {
        let local_content = local_agent_content_preview(storage, session_id)?;
        if !local_content.is_empty() {
            return Ok(GroupTurnResult::BoundaryConfirmationNeeded {
                error_code: "E6004",
                preview_content: local_content.join("\n---\n"),
            });
        }
    }

    let mut history = build_group_history_for_speaker(storage, session_id, &speaker_id)?;
    if let Some(system_prompt) = &agent.system_prompt {
        history.insert(0, ChatMessage { role: "system".to_string(), content: system_prompt.clone() });
    }

    let reply = agent_manager::send_message(storage, &agent, &history).map_err(|e| e.to_string())?;

    // E9004 (see guardrails::screen_agent_reply_for_impersonation): advisory
    // only, per the Error Code Registry's defined handling for this code —
    // log and keep the agent's real identity, don't block the reply.
    let other_names: Vec<String> = member_ids
        .iter()
        .filter(|id| **id != speaker_id)
        .filter_map(|id| storage.get_agent(id).ok().flatten())
        .map(|a| a.name)
        .collect();
    if let Some(violation) = guardrails::screen_agent_reply_for_impersonation(&reply, &other_names) {
        eprintln!("guardrails: {violation} (agent {} in session {session_id})", agent.name);
    }

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

    Ok(GroupTurnResult::Message(saved))
}

/// A user message in a Group Chat: persists it, resets the loop
/// safety-net (a real user spoke), then runs exactly one agent turn —
/// the `@mentioned` agent if any, otherwise whoever is next in rotation.
#[tauri::command]
pub fn send_group_message(
    storage: State<Storage>,
    session_id: String,
    content: String,
) -> Result<GroupTurnResult, String> {
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
pub fn advance_group_turn(storage: State<Storage>, session_id: String) -> Result<GroupTurnResult, String> {
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
) -> Result<GroupTurnResult, String> {
    let members = session_members(&storage, &session_id)?;
    let summarizer = orchestrator::pick_summarizer(&members, summarizer_agent_id.as_deref())
        .cloned()
        .ok_or_else(|| GroupChatError::NoMembers.to_string())?;

    if summarizer.provider_kind == "cloud" && !storage.has_local_to_cloud_consent(&session_id).map_err(|e| e.to_string())? {
        let local_content = local_agent_content_preview(&storage, &session_id)?;
        if !local_content.is_empty() {
            return Ok(GroupTurnResult::BoundaryConfirmationNeeded {
                error_code: "E6004",
                preview_content: local_content.join("\n---\n"),
            });
        }
    }

    let mut history = build_group_history_for_speaker(&storage, &session_id, &summarizer.id)?;
    history.push(ChatMessage {
        role: "user".to_string(),
        content: "Please summarize this meeting: the key points discussed, any decisions reached, and any open questions left for the user.".to_string(),
    });
    if let Some(system_prompt) = &summarizer.system_prompt {
        history.insert(0, ChatMessage { role: "system".to_string(), content: system_prompt.clone() });
    }

    let summary = agent_manager::send_message(&storage, &summarizer, &history).map_err(|e| e.to_string())?;

    let saved = storage
        .add_message(&session_id, Some(&summarizer.id), "assistant", &summary)
        .map_err(|e| e.to_string())?;

    Ok(GroupTurnResult::Message(saved))
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

/// Same picker-only-creation rule as `grant_folder_access` — the folder
/// still comes from a real OS dialog the user just drove. The only
/// difference is the grant is shared with every agent currently in
/// `session_id` (a Group Chat), not just `agent_id` — see
/// `storage::grant_folder_access_for_session`.
#[tauri::command]
pub fn grant_folder_access_for_session(
    storage: State<Storage>,
    session_id: String,
    agent_id: String,
    folder_path: String,
) -> Result<FileAccessGrant, String> {
    storage
        .grant_folder_access_for_session(&session_id, &agent_id, &folder_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_file_access_grants(storage: State<Storage>, agent_id: String) -> Result<Vec<FileAccessGrant>, String> {
    storage.list_file_access_grants(&agent_id).map_err(|e| e.to_string())
}

/// The whole Group Chat's shared folders, for that session's "Files" UI
/// — as opposed to `list_file_access_grants`, which only shows one
/// agent's own (private or session-granted-by-them) rows.
#[tauri::command]
pub fn list_session_shared_file_grants(
    storage: State<Storage>,
    session_id: String,
) -> Result<Vec<FileAccessGrant>, String> {
    storage.list_session_shared_file_grants(&session_id).map_err(|e| e.to_string())
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

/// Edits an existing custom template in place. Does not touch any Agent
/// created from this template before the edit — `Agent::system_prompt`
/// was copied at creation time, so this only changes what a *future*
/// "apply this template" picks up (see `storage::update_custom_role_template`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_custom_role_template(
    storage: State<Storage>,
    id: String,
    name: String,
    description: String,
    system_prompt: String,
    suggested_provider_kind: Option<String>,
    suggested_provider_name: Option<String>,
    suggested_model: Option<String>,
) -> Result<RoleTemplate, String> {
    storage
        .update_custom_role_template(
            &id,
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

/// Writes one custom role template to `dest_path` as pretty-printed JSON,
/// stripped of its database `id` (see `RoleTemplateExport`) so the file is
/// meaningful when imported into a different install/database.
#[tauri::command]
pub fn export_custom_role_template(storage: State<Storage>, id: String, dest_path: String) -> Result<(), String> {
    let template = storage
        .list_custom_role_templates()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(RoleTemplate::from)
        .find(|t| t.id == id)
        .ok_or_else(|| format!("no custom role template with id {id}"))?;
    let export = role_templates::RoleTemplateExport::from(&template);
    let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
    std::fs::write(&dest_path, json).map_err(|e| format!("failed to write {dest_path}: {e}"))
}

/// Reads a role template JSON file (as produced by `export_custom_role_template`)
/// and creates it as a brand-new custom template — always a fresh id, even
/// if the file was exported from this same install.
#[tauri::command]
pub fn import_custom_role_template(storage: State<Storage>, source_path: String) -> Result<RoleTemplate, String> {
    let text = std::fs::read_to_string(&source_path).map_err(|e| format!("failed to read {source_path}: {e}"))?;
    let import: role_templates::RoleTemplateExport =
        serde_json::from_str(&text).map_err(|e| format!("invalid role template file: {e}"))?;
    storage
        .create_custom_role_template(
            &import.name,
            &import.description,
            &import.system_prompt,
            import.suggested_provider_kind.as_deref(),
            import.suggested_provider_name.as_deref(),
            import.suggested_model.as_deref(),
        )
        .map(RoleTemplate::from)
        .map_err(|e| e.to_string())
}

// --- Skills (Python bridge) ---

/// Merges the bundled (built-in) and user-writable (custom) skills
/// directories into one list, each manifest tagged with where it came
/// from — mirrors how Role Templates merge built-in Rust consts with
/// user-authored ones into a single list for the UI.
#[tauri::command]
pub fn list_skills(skill_dirs: State<SkillDirs>) -> Vec<SkillManifest> {
    let mut manifests = skill_manager::discover_skills_tagged(&skill_dirs.builtin, "built-in");
    manifests.extend(skill_manager::discover_skills_tagged(&skill_dirs.custom, "custom"));
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    manifests
}

/// Copies a user-picked folder (containing `skill.json` + its entrypoint)
/// into the custom-skills directory, then restarts the skill bridge so
/// the newly added skill becomes callable without restarting the app.
/// Validation is deliberately minimal (manifest parses, entrypoint file
/// exists) — see [[Unified Data Folder & Custom Skills Design]] for why
/// deeper code-safety scanning is explicitly out of scope for this pass,
/// and why importing a skill carries the same trust level as a built-in
/// one (no per-skill sandboxing exists yet).
#[tauri::command]
pub fn import_custom_skill(
    app: tauri::AppHandle,
    skill_dirs: State<SkillDirs>,
    skill_runtime: State<SkillRuntimeState>,
    source_folder: String,
) -> Result<SkillManifest, String> {
    let source = std::path::Path::new(&source_folder);
    let manifest_text = std::fs::read_to_string(source.join("skill.json"))
        .map_err(|e| format!("skill.json not found or unreadable in {source_folder}: {e}"))?;
    let manifest: SkillManifest =
        serde_json::from_str(&manifest_text).map_err(|e| format!("invalid skill.json: {e}"))?;
    if !source.join(&manifest.entrypoint).exists() {
        return Err(format!("entrypoint \"{}\" not found in {source_folder}", manifest.entrypoint));
    }

    let dest = skill_dirs.custom.join(&manifest.name);
    if dest.exists() {
        return Err(format!("a custom skill named \"{}\" already exists — remove it first to re-import", manifest.name));
    }
    skill_manager::copy_dir_recursive(source, &dest).map_err(|e| format!("failed to copy skill files: {e}"))?;

    // Best-effort restart, same policy as the initial app-startup spawn:
    // a missing/broken Python install shouldn't stop the rest of the app,
    // it just means Skills (including the one just imported) stay
    // unavailable until the underlying problem is fixed.
    let bundled_python = crate::bridge_support::find_bundled_python(app.path().resource_dir().ok().as_deref());
    let mut guard = skill_runtime.0.lock().unwrap();
    *guard =
        skill_manager::SkillRuntime::start(&[skill_dirs.builtin.clone(), skill_dirs.custom.clone()], bundled_python.as_deref())
            .ok();

    Ok(SkillManifest { source: "custom".to_string(), ..manifest })
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

// --- MCP (Model Context Protocol) client — mirrors the Skills command
// block above exactly: add/list/delete a server, grant/list/revoke
// per-agent access, then the two dispatch entry points. See
// mcp_manager module docs for the enforcement-order guarantee
// (Guardrails, then the allowlist) these commands rely on.

#[tauri::command]
pub fn add_mcp_server(storage: State<Storage>, name: String, command: String, args: Vec<String>) -> Result<McpServer, String> {
    storage.create_mcp_server(&name, &command, &args).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_mcp_servers(storage: State<Storage>) -> Result<Vec<McpServer>, String> {
    storage.list_mcp_servers().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_mcp_server(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.delete_mcp_server(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn grant_mcp_access(storage: State<Storage>, agent_id: String, mcp_server_id: String) -> Result<McpAccessGrant, String> {
    storage.grant_mcp_access(&agent_id, &mcp_server_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_mcp_access_grants(storage: State<Storage>, agent_id: String) -> Result<Vec<McpAccessGrant>, String> {
    storage.list_mcp_access_grants(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revoke_mcp_access(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.revoke_mcp_access_grant(&id).map_err(|e| e.to_string())
}

/// Connects to `mcp_server_id` and returns its tools, already screened
/// for tool-poisoning (see `mcp_manager::list_tools_screened`) — this is
/// a live network/subprocess call, not a cached list, since this module
/// doesn't keep a persistent connection open per server (see mcp_manager
/// module docs).
#[tauri::command]
pub async fn list_mcp_server_tools(storage: State<'_, Storage>, mcp_server_id: String) -> Result<Vec<McpTool>, String> {
    let server = storage
        .get_mcp_server(&mcp_server_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("E4001 MCP server \"{mcp_server_id}\" not found"))?;
    let config = mcp_manager::McpServerConfig { command: server.command, args: server.args };
    mcp_manager::list_tools_screened(&config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn call_mcp_tool(
    storage: State<'_, Storage>,
    agent_id: String,
    mcp_server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<String, String> {
    mcp_manager::invoke_mcp_tool(&storage, &agent_id, &mcp_server_id, &tool_name, arguments)
        .await
        .map_err(|e| e.to_string())
}

/// Runs an MCP tool on `agent_id`'s behalf and persists the result into
/// `session_id`'s message history (`role: "system"`), the same pattern
/// as `run_skill_in_session` — visible in the transcript, not
/// attributed to "user" or "assistant".
#[tauri::command]
pub async fn run_mcp_tool_in_session(
    storage: State<'_, Storage>,
    session_id: String,
    agent_id: String,
    mcp_server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<Message, String> {
    let result = mcp_manager::invoke_mcp_tool(&storage, &agent_id, &mcp_server_id, &tool_name, arguments)
        .await
        .map_err(|e| e.to_string())?;
    let content = format!("[MCP tool \"{tool_name}\" result]\n{result}");
    storage.add_message(&session_id, Some(&agent_id), "system", &content).map_err(|e| e.to_string())
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

// --- ML Engine (semantic search / RAG) ---
//
// See `ml_engine` module docs and the vault's `ML Engine Design.md` — a
// separate Python subprocess from the Skills bridge, gated by the same
// Guardrails-then-allowlist pattern, but with its own grant scoping
// (`ml_access_grants`) that supports Group-Chat-session-shared access.

#[tauri::command]
pub fn list_ml_capabilities(ml_dir: State<MlDir>) -> Vec<MlCapabilityManifest> {
    ml_engine::discover_capabilities(&ml_dir.0)
}

#[tauri::command]
pub fn grant_ml_capability_to_agent(
    storage: State<Storage>,
    agent_id: String,
    capability_name: String,
) -> Result<MlAccessGrant, String> {
    storage.grant_ml_capability("agent", &agent_id, &capability_name).map_err(|e| e.to_string())
}

/// Grants a capability to an entire Group Chat session — every agent
/// currently in that session gets access, and it's checked live against
/// current membership (`storage::has_ml_capability_access`), not a
/// snapshot taken now.
#[tauri::command]
pub fn grant_ml_capability_to_session(
    storage: State<Storage>,
    session_id: String,
    capability_name: String,
) -> Result<MlAccessGrant, String> {
    storage.grant_ml_capability("session", &session_id, &capability_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revoke_ml_access_grant(storage: State<Storage>, id: String) -> Result<(), String> {
    storage.revoke_ml_access_grant(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_ml_access_grants_for_agent(storage: State<Storage>, agent_id: String) -> Result<Vec<MlAccessGrant>, String> {
    storage.list_ml_access_grants_for_agent(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_ml_access_grants_for_session(
    storage: State<Storage>,
    session_id: String,
) -> Result<Vec<MlAccessGrant>, String> {
    storage.list_ml_access_grants_for_session(&session_id).map_err(|e| e.to_string())
}

/// Rebuilds `index_name` from every `.md`/`.txt` file across `agent_id`'s
/// own granted folders (private grants only — see
/// `build_semantic_index_for_session` for a Group Chat's shared index,
/// which deliberately does **not** reuse this function to avoid folding
/// the acting agent's private folders into an index every meeting member
/// can search). The Rust side reads the files (respecting File Access
/// grants — the capability itself never receives a folder path to read
/// on its own), then hands the text to `semantic_search`'s `index`
/// action through the normal Guardrails + allowlist gate.
#[tauri::command]
pub fn build_semantic_index(
    storage: State<Storage>,
    runtime: State<MlEngineRuntimeState>,
    agent_id: String,
    index_name: String,
) -> Result<serde_json::Value, String> {
    let documents = file_access::list_text_files_in_grants(&storage, &agent_id);
    if documents.is_empty() {
        return Err(
            "no indexable .md/.txt files found in this agent's granted folders — grant a folder first".to_string(),
        );
    }
    let payload = serde_json::json!({
        "action": "index",
        "indexName": index_name,
        "documents": documents
            .into_iter()
            .map(|(path, text)| serde_json::json!({"path": path, "text": text}))
            .collect::<Vec<_>>(),
    });
    let guard = runtime.0.lock().unwrap();
    ml_engine::invoke(&storage, guard.as_ref(), &agent_id, "semantic_search", payload).map_err(|e| e.to_string())
}

/// The Group Chat counterpart of `build_semantic_index`: rebuilds
/// `index_name` (by convention `group-<sessionId>`, see `ML Engine
/// Design.md` §4.1) from only the folders explicitly shared to
/// `session_id` (`file_access::list_text_files_in_session_grants`) — not
/// any single member's private grants. `agent_id` is used purely for the
/// Guardrails/allowlist gate (it must be authorized, directly or via a
/// session-scope `ml_access_grant`), not for picking which files to read.
#[tauri::command]
pub fn build_semantic_index_for_session(
    storage: State<Storage>,
    runtime: State<MlEngineRuntimeState>,
    session_id: String,
    agent_id: String,
    index_name: String,
) -> Result<serde_json::Value, String> {
    let documents = file_access::list_text_files_in_session_grants(&storage, &session_id);
    if documents.is_empty() {
        return Err(
            "no indexable .md/.txt files shared with this meeting — grant a folder first".to_string(),
        );
    }
    let payload = serde_json::json!({
        "action": "index",
        "indexName": index_name,
        "documents": documents
            .into_iter()
            .map(|(path, text)| serde_json::json!({"path": path, "text": text}))
            .collect::<Vec<_>>(),
    });
    let guard = runtime.0.lock().unwrap();
    ml_engine::invoke(&storage, guard.as_ref(), &agent_id, "semantic_search", payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn semantic_search_query(
    storage: State<Storage>,
    runtime: State<MlEngineRuntimeState>,
    agent_id: String,
    index_name: String,
    query: String,
    top_k: Option<u32>,
) -> Result<serde_json::Value, String> {
    let payload = serde_json::json!({
        "action": "search",
        "indexName": index_name,
        "query": query,
        "topK": top_k.unwrap_or(5),
    });
    let guard = runtime.0.lock().unwrap();
    ml_engine::invoke(&storage, guard.as_ref(), &agent_id, "semantic_search", payload).map_err(|e| e.to_string())
}

/// Checks GitHub for a newer release than `current_version` (blocking
/// HTTP call, run on a background thread by `spawn_blocking` since this
/// is a plain `#[tauri::command]`, not `async fn`, and `reqwest::blocking`
/// would otherwise stall the Tauri IPC executor). Never installs
/// anything — the frontend links the user to `releaseUrl` to download
/// manually, since code signing (needed for `tauri-plugin-updater`'s
/// silent-update path) is deferred to the Beta stage.
#[tauri::command]
pub async fn check_for_update(current_version: String) -> Result<update_check::UpdateCheckResult, String> {
    tauri::async_runtime::spawn_blocking(move || update_check::check_for_update(&current_version))
        .await
        .map_err(|e| e.to_string())?
}

/// One node of a task DAG as submitted over Tauri IPC — see
/// `orchestrator::dag`. `agent_id` is looked up against `Storage` here
/// (not resolved by the frontend) so a stale/deleted agent id surfaces
/// as the same `DagError::UnknownAgent` the module already handles,
/// rather than a separate frontend-side failure mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNodeInput {
    pub id: String,
    pub agent_id: String,
    pub prompt: String,
    pub depends_on: Vec<String>,
}

/// Runs a task DAG (`orchestrator::dag`) end to end: validates the graph
/// shape, resolves every referenced `agent_id` against `Storage` up
/// front, then dispatches each node in dependency order through
/// `agent_manager::send_message` — the same Guardrails-gated path every
/// other message takes. Returns every node's output keyed by node id;
/// the frontend decides what to do with a multi-node result (e.g. show
/// each node's output in its own card).
#[tauri::command]
pub fn run_task_dag(storage: State<Storage>, nodes: Vec<TaskNodeInput>) -> Result<std::collections::HashMap<String, String>, String> {
    let dag = orchestrator::dag::TaskDag {
        nodes: nodes
            .into_iter()
            .map(|n| orchestrator::dag::TaskNode { id: n.id, agent_id: n.agent_id, prompt: n.prompt, depends_on: n.depends_on })
            .collect(),
    };

    let agent_ids: std::collections::HashSet<&str> = dag.nodes.iter().map(|n| n.agent_id.as_str()).collect();
    let mut agents = std::collections::HashMap::new();
    for id in agent_ids {
        let agent = storage.get_agent(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("agent {id} not found"))?;
        agents.insert(id.to_string(), agent);
    }

    orchestrator::dag::run(&storage, &dag, &agents).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the E6004 boundary check actually blocks a cloud call
    /// before it happens, not just that the logic exists — no real
    /// network/provider call occurs in this test, because a correct
    /// implementation returns `BoundaryConfirmationNeeded` before ever
    /// reaching `agent_manager::send_message`. Local content is seeded
    /// directly into storage (bypassing any real Ollama call) so this
    /// test needs no local model, no API key, and no network access —
    /// it can run in every `cargo test`, not just `-- --ignored`.
    #[test]
    fn cloud_speaker_with_unconfirmed_local_content_pauses_instead_of_calling_the_provider() {
        let storage = Storage::open_in_memory().unwrap();
        let local_agent =
            storage.create_agent("Local", None, None, "local", "ollama", "llama3.1:8b").unwrap();
        let cloud_agent = storage
            .create_agent("Cloud", None, None, "cloud", "openrouter", "some/model")
            .unwrap();

        let session = storage.create_session("group", "Boundary test").unwrap();
        storage.add_agent_to_session(&session.id, &local_agent.id).unwrap();
        storage.add_agent_to_session(&session.id, &cloud_agent.id).unwrap();

        // Seed the local agent's "turn" directly — no real Ollama call.
        storage.add_message(&session.id, Some(&local_agent.id), "assistant", "local agent's output").unwrap();
        // Point rotation at the cloud agent (index 1) so this resolves to
        // it directly, without needing to actually run the local agent's
        // turn through send_message.
        storage
            .save_group_session_state(&crate::storage::GroupSessionState {
                session_id: session.id.clone(),
                rotation_cursor: 1,
                consecutive_agent_turns: 0,
            })
            .unwrap();

        let result = run_one_group_turn(&storage, &session.id, None).unwrap();
        match result {
            GroupTurnResult::BoundaryConfirmationNeeded { error_code, preview_content } => {
                assert_eq!(error_code, "E6004");
                assert!(preview_content.contains("local agent's output"));
            }
            GroupTurnResult::Message(_) => panic!("expected a boundary pause, got a completed turn"),
        }

        // No turn-taking state should have advanced — retrying after
        // confirmation must resolve the exact same speaker.
        let state = storage.get_group_session_state(&session.id).unwrap();
        assert_eq!(state.rotation_cursor, 1);
        assert_eq!(state.consecutive_agent_turns, 0);
    }

    #[test]
    fn confirming_the_boundary_lets_the_same_turn_proceed_past_the_check() {
        let storage = Storage::open_in_memory().unwrap();
        let local_agent =
            storage.create_agent("Local", None, None, "local", "ollama", "llama3.1:8b").unwrap();
        let cloud_agent = storage
            .create_agent("Cloud", None, None, "cloud", "openrouter", "some/model")
            .unwrap();

        let session = storage.create_session("group", "Boundary confirm test").unwrap();
        storage.add_agent_to_session(&session.id, &local_agent.id).unwrap();
        storage.add_agent_to_session(&session.id, &cloud_agent.id).unwrap();
        storage.add_message(&session.id, Some(&local_agent.id), "assistant", "local agent's output").unwrap();
        storage
            .save_group_session_state(&crate::storage::GroupSessionState {
                session_id: session.id.clone(),
                rotation_cursor: 1,
                consecutive_agent_turns: 0,
            })
            .unwrap();

        assert!(matches!(
            run_one_group_turn(&storage, &session.id, None).unwrap(),
            GroupTurnResult::BoundaryConfirmationNeeded { .. }
        ));

        storage.grant_local_to_cloud_consent(&session.id).unwrap();

        // The check itself is now satisfied — this would go on to call
        // the real cloud provider next (which fails here because
        // "some/model" isn't real and there's no API key), proving the
        // *boundary check* no longer blocks it; the resulting provider
        // error is expected and irrelevant to what this test verifies.
        let after_confirm = run_one_group_turn(&storage, &session.id, None);
        assert!(
            !matches!(after_confirm, Ok(GroupTurnResult::BoundaryConfirmationNeeded { .. })),
            "confirming consent should let the turn past the boundary check, got: {after_confirm:?}"
        );
    }
}

/// Live end-to-end tests exercising the internal (`&Storage`-only, no
/// `tauri::State`) Group Chat functions against real OpenRouter agents.
/// Not run in CI (needs a real free API key) — run manually with
/// `cargo test commands::live -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;
    use crate::key_vault;

    /// These live tests only ever exercise all-cloud sessions (no local
    /// Agent involved), so `run_one_group_turn` never has anything to
    /// pause for — this just unwraps the `Message` variant so the tests
    /// below can keep asserting on `.agent_id`/`.content` directly.
    fn expect_message(result: GroupTurnResult) -> Message {
        match result {
            GroupTurnResult::Message(message) => message,
            GroupTurnResult::BoundaryConfirmationNeeded { .. } => {
                panic!("expected a completed turn, got a boundary confirmation pause")
            }
        }
    }

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
        let first_reply = expect_message(run_one_group_turn(&storage, &session.id, None).unwrap());
        assert_eq!(first_reply.agent_id.as_deref(), Some(alice.id.as_str()));
        assert!(first_reply.content.to_uppercase().contains("ALICE"), "got: {}", first_reply.content);

        // Then Bob, in rotation order.
        let second_reply = expect_message(run_one_group_turn(&storage, &session.id, None).unwrap());
        assert_eq!(second_reply.agent_id.as_deref(), Some(bob.id.as_str()));
        assert!(second_reply.content.to_uppercase().contains("BOB"), "got: {}", second_reply.content);

        // An @mention pulls Alice back in out of turn.
        let mentioned_reply = expect_message(run_one_group_turn(&storage, &session.id, Some(&alice.id)).unwrap());
        assert_eq!(mentioned_reply.agent_id.as_deref(), Some(alice.id.as_str()));

        // Rotation resumes where it left off: Alice (index 0) was next before
        // the mention, so it should still be Alice's turn now.
        let fourth_reply = expect_message(run_one_group_turn(&storage, &session.id, None).unwrap());
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
        let runtime = skill_manager::SkillRuntime::start(std::slice::from_ref(&skills_dir), None)
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
