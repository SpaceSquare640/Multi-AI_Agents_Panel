mod agent_manager;
mod commands;
mod fallback;
mod file_access;
mod guardrails;
mod key_vault;
mod ml_engine;
mod orchestrator;
mod session_manager;
mod skill_manager;
mod storage;
mod usage_tracker;

use std::sync::Mutex;

use ml_engine::MlEngineRuntime;
use skill_manager::SkillRuntime;
use storage::Storage;
use tauri::Manager;

/// Tauri managed state wrapping the optional Python skill bridge — `None`
/// when no working Python interpreter was found or the bridge failed to
/// start (Skills are then unavailable, but the rest of the app still
/// works; see `skill_manager::SkillRuntime`).
pub(crate) struct SkillRuntimeState(pub(crate) Mutex<Option<SkillRuntime>>);

/// The resolved `skills/` directory, stashed as managed state so commands
/// can re-scan it (`list_skills`) without re-deriving the path each time.
pub(crate) struct SkillsDir(pub(crate) std::path::PathBuf);

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

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("data.sqlite3");
            let storage = Storage::open(&db_path)
                .map_err(|e| format!("failed to open storage at {:?}: {e}", db_path))?;
            app.manage(storage);

            let resource_dir = app.path().resource_dir().ok();
            let skills_dir = skill_manager::resolve_skills_dir(resource_dir);
            // Best-effort: a missing/broken Python install shouldn't stop
            // the rest of the app from working, only Skills.
            let runtime = match SkillRuntime::start(&skills_dir) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    eprintln!("skill bridge unavailable: {e}");
                    None
                }
            };
            app.manage(SkillRuntimeState(Mutex::new(runtime)));
            app.manage(SkillsDir(skills_dir));

            let ml_dir = ml_engine::resolve_ml_dir(app.path().resource_dir().ok());
            // Same best-effort policy as the Skills bridge: a missing
            // Python/sentence-transformers install shouldn't stop the
            // rest of the app from working, only ML capabilities.
            let ml_runtime = match MlEngineRuntime::start(&ml_dir) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    eprintln!("ML engine bridge unavailable: {e}");
                    None
                }
            };
            app.manage(MlEngineRuntimeState(Mutex::new(ml_runtime)));
            app.manage(MlDir(ml_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::list_provider_keys,
            commands::add_provider_key,
            commands::batch_add_provider_keys,
            commands::delete_provider_key,
            commands::get_usage_summary,
            commands::list_curated_models,
            commands::ollama_is_running,
            commands::list_ollama_installed_models,
            commands::pull_ollama_model,
            commands::delete_ollama_model,
            commands::list_agents,
            commands::create_agent,
            commands::pin_agent_provider_key,
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
            commands::delete_custom_role_template,
            commands::list_skills,
            commands::grant_skill_access,
            commands::list_skill_access_grants,
            commands::revoke_skill_access,
            commands::invoke_skill,
            commands::run_skill_in_session,
            commands::create_group_session,
            commands::list_session_members,
            commands::send_group_message,
            commands::advance_group_turn,
            commands::end_group_chat_meeting,
            commands::pull_out_to_independent_session,
            commands::list_ml_capabilities,
            commands::grant_ml_capability_to_agent,
            commands::grant_ml_capability_to_session,
            commands::revoke_ml_access_grant,
            commands::list_ml_access_grants_for_agent,
            commands::list_ml_access_grants_for_session,
            commands::build_semantic_index,
            commands::semantic_search_query,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
