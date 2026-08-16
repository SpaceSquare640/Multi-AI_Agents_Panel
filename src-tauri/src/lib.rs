mod agent_manager;
mod commands;
mod fallback;
mod file_access;
mod game_agent;
mod guardrails;
mod key_vault;
mod ml_engine;
mod orchestrator;
mod session_manager;
mod skill_manager;
mod storage;
mod usage_tracker;

use std::sync::Mutex;

use agent_manager::openrouter_catalog::OpenRouterCatalogState;
use game_agent::{GameAgentState, RecordingState};
use ml_engine::MlEngineRuntime;
use skill_manager::SkillRuntime;
use storage::Storage;
use tauri::Manager;

/// Tauri managed state wrapping the optional Python skill bridge — `None`
/// when no working Python interpreter was found or the bridge failed to
/// start (Skills are then unavailable, but the rest of the app still
/// works; see `skill_manager::SkillRuntime`).
pub(crate) struct SkillRuntimeState(pub(crate) Mutex<Option<SkillRuntime>>);

/// The two directories `list_skills`/`import_custom_skill` work with:
/// `builtin` is the bundled, read-only resource directory (refreshed by
/// the installer on every app update); `custom` is the user-writable
/// directory under the unified data folder (see `resolve_data_dir`) that
/// survives app updates/reinstalls. See
/// `Unified Data Folder & Custom Skills Design.md` in the vault for why
/// these are deliberately two separate directories rather than one.
pub(crate) struct SkillDirs {
    pub(crate) builtin: std::path::PathBuf,
    pub(crate) custom: std::path::PathBuf,
}

/// Tauri managed state wrapping the optional Python `ml_engine` bridge —
/// a separate process from `SkillRuntimeState`, see `ml_engine` module
/// docs for why. `None` when no working Python interpreter was found or
/// the bridge failed to start (ML capabilities are then unavailable, but
/// the rest of the app still works).
pub(crate) struct MlEngineRuntimeState(pub(crate) Mutex<Option<MlEngineRuntime>>);

/// The resolved `ml/` directory, stashed as managed state so commands can
/// re-scan it (`list_ml_capabilities`) without re-deriving the path each
/// time.
pub(crate) struct MlDir(pub(crate) std::path::PathBuf);

/// The resolved `game_agent_rl/` directory — Track B's standalone Python
/// CLI tooling, see `game_agent::resolve_recording_dir`.
pub(crate) struct RecordingDir(pub(crate) std::path::PathBuf);

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Resolves the unified data folder: `data.sqlite3`, the ML model cache,
/// and user-imported custom Skills all live under here. Deliberately a
/// fixed, cross-platform-consistent folder under the user's home
/// directory (`~/MultiAIAgentsPanel-Data/`) rather than Tauri's default
/// `app_data_dir()` (which scatters across `%APPDATA%`, `~/Library/...`,
/// `~/.local/share/...` depending on OS) or the install directory itself
/// (installer upgrade/reinstall behavior touching that path has never
/// been verified, and deb/AppImage have no equivalent writable "install
/// directory" concept at all). See ADR 0004 and
/// `Unified Data Folder & Custom Skills Design.md` in the vault for the
/// full reasoning.
fn resolve_data_dir(home_dir: std::path::PathBuf) -> std::path::PathBuf {
    home_dir.join("MultiAIAgentsPanel-Data")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = resolve_data_dir(app.path().home_dir()?);
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("data.sqlite3");
            let storage = Storage::open(&db_path)
                .map_err(|e| format!("failed to open storage at {:?}: {e}", db_path))?;
            app.manage(storage);

            let resource_dir = app.path().resource_dir().ok();
            let builtin_skills_dir = skill_manager::resolve_skills_dir(resource_dir);
            let custom_skills_dir = data_dir.join("skills");
            std::fs::create_dir_all(&custom_skills_dir)?;
            // Best-effort: a missing/broken Python install shouldn't stop
            // the rest of the app from working, only Skills.
            let runtime = match SkillRuntime::start(&[builtin_skills_dir.clone(), custom_skills_dir.clone()]) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    eprintln!("skill bridge unavailable: {e}");
                    None
                }
            };
            app.manage(SkillRuntimeState(Mutex::new(runtime)));
            app.manage(SkillDirs { builtin: builtin_skills_dir, custom: custom_skills_dir });

            let ml_dir = ml_engine::resolve_ml_dir(app.path().resource_dir().ok());
            let ml_cache_dir = data_dir.join("ml-cache");
            std::fs::create_dir_all(&ml_cache_dir)?;
            // Same best-effort policy as the Skills bridge: a missing
            // Python/sentence-transformers install shouldn't stop the
            // rest of the app from working, only ML capabilities.
            let ml_runtime = match MlEngineRuntime::start(&ml_dir, Some(&ml_cache_dir)) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    eprintln!("ML engine bridge unavailable: {e}");
                    None
                }
            };
            app.manage(MlEngineRuntimeState(Mutex::new(ml_runtime)));
            app.manage(MlDir(ml_dir));
            app.manage(OpenRouterCatalogState(Mutex::new(None)));
            // Starts every launch stopped — the game agent never runs
            // unless a user explicitly clicks "start" (see game_agent
            // module docs).
            app.manage(GameAgentState(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))));
            app.manage(RecordingState(Mutex::new(None)));
            app.manage(RecordingDir(game_agent::resolve_recording_dir(app.path().resource_dir().ok())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::list_provider_keys,
            commands::add_provider_key,
            commands::batch_add_provider_keys,
            commands::import_provider_keys_from_files,
            commands::delete_provider_key,
            commands::get_usage_summary,
            commands::list_curated_models,
            commands::list_openrouter_models_live,
            commands::start_game_agent,
            commands::stop_game_agent,
            commands::game_agent_status,
            commands::start_recording_session,
            commands::stop_recording_session,
            commands::recording_status,
            commands::ollama_is_running,
            commands::list_ollama_installed_models,
            commands::pull_ollama_model,
            commands::delete_ollama_model,
            commands::ollama_models_env_hint,
            commands::list_agents,
            commands::create_agent,
            commands::pin_agent_provider_key,
            commands::add_agent_fallback_provider,
            commands::list_agent_fallback_providers,
            commands::remove_agent_fallback_provider,
            commands::list_sessions,
            commands::create_independent_session,
            commands::list_messages,
            commands::get_session_agent_id,
            commands::send_chat_message,
            commands::grant_folder_access,
            commands::grant_folder_access_for_session,
            commands::list_file_access_grants,
            commands::list_session_shared_file_grants,
            commands::revoke_file_access_grant,
            commands::list_default_role_templates,
            commands::list_custom_role_templates,
            commands::create_custom_role_template,
            commands::update_custom_role_template,
            commands::delete_custom_role_template,
            commands::export_custom_role_template,
            commands::import_custom_role_template,
            commands::list_skills,
            commands::import_custom_skill,
            commands::grant_skill_access,
            commands::list_skill_access_grants,
            commands::revoke_skill_access,
            commands::invoke_skill,
            commands::run_skill_in_session,
            commands::create_group_session,
            commands::list_session_members,
            commands::send_group_message,
            commands::advance_group_turn,
            commands::confirm_local_to_cloud_boundary,
            commands::end_group_chat_meeting,
            commands::pull_out_to_independent_session,
            commands::list_ml_capabilities,
            commands::grant_ml_capability_to_agent,
            commands::grant_ml_capability_to_session,
            commands::revoke_ml_access_grant,
            commands::list_ml_access_grants_for_agent,
            commands::list_ml_access_grants_for_session,
            commands::build_semantic_index,
            commands::build_semantic_index_for_session,
            commands::semantic_search_query,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_data_dir_is_a_fixed_subfolder_of_home_not_an_os_default_app_data_path() {
        let home = std::path::PathBuf::from("/home/someone");
        let data_dir = resolve_data_dir(home.clone());
        assert_eq!(data_dir, home.join("MultiAIAgentsPanel-Data"));
    }
}
