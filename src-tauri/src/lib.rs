mod agent_manager;
mod bridge_support;
mod cherrytree;
mod commands;
mod fallback;
mod file_access;
mod game_agent;
mod guardrails;
mod key_vault;
mod mcp_manager;
mod ml_engine;
mod orchestrator;
mod session_manager;
mod skill_manager;
mod storage;
mod update_check;
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
/// survives app updates/reinstalls — these are deliberately two
/// separate directories rather than one.
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

/// Resolves the unified data root: `Data/data.sqlite3`, `LocalAIModel/`
/// (ML model cache), and `skill/` (user-imported custom Skills) all live
/// under here as named subfolders. Deliberately a fixed,
/// cross-platform-consistent folder under the user's home directory
/// (`~/Multi-AI Agents Panel/`) rather than Tauri's default
/// `app_data_dir()` (which scatters across `%APPDATA%`, `~/Library/...`,
/// `~/.local/share/...` depending on OS) or the install directory itself
/// (installer upgrade/reinstall behavior touching that path has never
/// been verified, and deb/AppImage have no equivalent writable "install
/// directory" concept at all). See ADR 0004 for the full reasoning.
///
/// **Named to match `productName` exactly** (with spaces) — previously
/// `~/MultiAIAgentsPanel/` (no spaces), which looked unrelated to the
/// `Multi-AI Agents Panel` folder the Windows installer creates, and was
/// a real source of user confusion about which folder was "the app's."
/// Unlike the earlier alpha-stage rename (`MultiAIAgentsPanel-Data` →
/// `MultiAIAgentsPanel`, done with no migration since nothing depended on
/// the old path yet), this app is GA now — real installs have real data
/// under the old name, so `migrate_data_dir` below moves it forward
/// instead of silently starting empty.
pub(crate) fn resolve_data_dir(home_dir: std::path::PathBuf) -> std::path::PathBuf {
    home_dir.join("Multi-AI Agents Panel")
}

/// One-time migration for the folder rename above: if the new
/// (space-containing) data dir doesn't exist yet but the old
/// (no-space) one does, moves it wholesale rather than leaving the old
/// one behind or starting the user over with an empty database. A
/// same-volume rename (the common case — both paths are under the same
/// home directory) is atomic and instant; `std::fs::rename` is used
/// directly rather than a recursive copy+delete, which would risk
/// leaving a half-copied mess if interrupted partway through. If the
/// rename fails for any reason (e.g. genuinely different volumes, a
/// file lock), this logs the failure and leaves the old directory in
/// place rather than losing data — the caller then creates a fresh
/// empty directory at the new path, same as a first launch.
fn migrate_data_dir(old_dir: &std::path::Path, new_dir: &std::path::Path) {
    if new_dir.exists() || !old_dir.exists() {
        return;
    }
    if let Err(e) = std::fs::rename(old_dir, new_dir) {
        eprintln!(
            "could not migrate old data folder {old_dir:?} to {new_dir:?}: {e} — leaving it in place, \
             starting fresh at the new location"
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let home_dir = app.path().home_dir()?;
            let data_dir = resolve_data_dir(home_dir.clone());
            migrate_data_dir(&home_dir.join("MultiAIAgentsPanel"), &data_dir);
            let db_dir = data_dir.join("Data");
            std::fs::create_dir_all(&db_dir)?;
            let db_path = db_dir.join("data.sqlite3");
            let storage = Storage::open(&db_path)
                .map_err(|e| format!("failed to open storage at {:?}: {e}", db_path))?;
            app.manage(storage);

            let resource_dir = app.path().resource_dir().ok();
            let builtin_skills_dir = skill_manager::resolve_skills_dir(resource_dir.clone());
            let custom_skills_dir = data_dir.join("skill");
            std::fs::create_dir_all(&custom_skills_dir)?;
            // Windows-only for now (see `bridge_support::find_bundled_python`);
            // `None` elsewhere falls back to searching `PATH`.
            let bundled_python = bridge_support::find_bundled_python(resource_dir.as_deref());
            // Best-effort: a missing/broken Python install shouldn't stop
            // the rest of the app from working, only Skills.
            let runtime = match SkillRuntime::start(
                &[builtin_skills_dir.clone(), custom_skills_dir.clone()],
                bundled_python.as_deref(),
            ) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    eprintln!("skill bridge unavailable: {e}");
                    None
                }
            };
            app.manage(SkillRuntimeState(Mutex::new(runtime)));
            app.manage(SkillDirs { builtin: builtin_skills_dir, custom: custom_skills_dir });

            let ml_dir = ml_engine::resolve_ml_dir(app.path().resource_dir().ok());
            let ml_cache_dir = data_dir.join("LocalAIModel");
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
            app.manage(game_agent::PlayState(Mutex::new(None)));
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
            commands::get_usage_summary_with_cost,
            commands::list_curated_models,
            commands::recommend_local_models,
            commands::list_openrouter_models_live,
            commands::start_game_agent,
            commands::stop_game_agent,
            commands::game_agent_status,
            commands::start_recording_session,
            commands::stop_recording_session,
            commands::recording_status,
            commands::label_recording_session,
            commands::train_behavior_cloning,
            commands::train_reinforcement,
            commands::start_play_checkpoint,
            commands::stop_play_checkpoint,
            commands::play_status,
            commands::ollama_is_running,
            commands::list_ollama_installed_models,
            commands::pull_ollama_model,
            commands::delete_ollama_model,
            commands::install_ollama,
            commands::install_cherrytree,
            commands::ollama_models_env_hint,
            commands::suggest_ollama_models_dir,
            commands::list_agents,
            commands::create_agent,
            commands::pin_agent_provider_key,
            commands::add_agent_fallback_provider,
            commands::list_agent_fallback_providers,
            commands::remove_agent_fallback_provider,
            commands::list_sessions,
            commands::delete_session,
            commands::list_notes,
            commands::create_note,
            commands::update_note,
            commands::delete_note,
            commands::create_independent_session,
            commands::list_messages,
            commands::get_session_agent_id,
            commands::send_chat_message,
            commands::send_chat_message_with_tools,
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
            commands::export_custom_skill,
            commands::grant_skill_access,
            commands::list_skill_access_grants,
            commands::revoke_skill_access,
            commands::invoke_skill,
            commands::run_skill_in_session,
            commands::add_mcp_server,
            commands::list_mcp_servers,
            commands::delete_mcp_server,
            commands::grant_mcp_access,
            commands::list_mcp_access_grants,
            commands::revoke_mcp_access,
            commands::list_mcp_server_tools,
            commands::check_for_update,
            commands::run_task_dag,
            commands::add_agent_memory,
            commands::list_agent_memories,
            commands::delete_agent_memory,
            commands::get_custom_instructions,
            commands::set_custom_instructions,
            commands::call_mcp_tool,
            commands::run_mcp_tool_in_session,
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            // Tauri's default window-close path exits the process directly
            // (no Rust `Drop` chain runs for managed state), so the
            // `SkillRuntime`/`MlEngineRuntime` child `python.exe` processes
            // they spawn at startup would otherwise survive the app
            // closing — an orphaned process holding the installed
            // `python-windows/*.pyd` files open, which is exactly what
            // makes a subsequent installer run fail with "Error opening
            // file for writing". `RunEvent::Exit` fires once, right before
            // the process actually terminates, giving us one real chance
            // to kill them explicitly instead of relying on `Drop`.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<SkillRuntimeState>() {
                    drop(state.0.lock().unwrap().take());
                }
                if let Some(state) = app_handle.try_state::<MlEngineRuntimeState>() {
                    drop(state.0.lock().unwrap().take());
                }
                // `record`/`play` subprocesses control real screen
                // capture and mouse/keyboard input — an orphaned one
                // surviving app close is worse than an orphaned bridge
                // process, so these get the same explicit-kill treatment.
                if let Some(state) = app_handle.try_state::<RecordingState>() {
                    if let Some(mut child) = state.0.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                }
                if let Some(state) = app_handle.try_state::<game_agent::PlayState>() {
                    if let Some(mut child) = state.0.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_data_dir_is_a_fixed_subfolder_of_home_not_an_os_default_app_data_path() {
        let home = std::path::PathBuf::from("/home/someone");
        let data_dir = resolve_data_dir(home.clone());
        assert_eq!(data_dir, home.join("Multi-AI Agents Panel"));
    }

    #[test]
    fn migrate_data_dir_moves_an_existing_old_folder_to_the_new_path() {
        let tmp = std::env::temp_dir().join(format!("mig-test-{}", std::process::id()));
        let old_dir = tmp.join("MultiAIAgentsPanel");
        let new_dir = tmp.join("Multi-AI Agents Panel");
        std::fs::create_dir_all(old_dir.join("Data")).unwrap();
        std::fs::write(old_dir.join("Data").join("marker.txt"), b"hello").unwrap();

        migrate_data_dir(&old_dir, &new_dir);

        assert!(!old_dir.exists());
        assert_eq!(std::fs::read_to_string(new_dir.join("Data").join("marker.txt")).unwrap(), "hello");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn migrate_data_dir_does_nothing_when_the_new_folder_already_exists() {
        let tmp = std::env::temp_dir().join(format!("mig-test-noop-{}", std::process::id()));
        let old_dir = tmp.join("MultiAIAgentsPanel");
        let new_dir = tmp.join("Multi-AI Agents Panel");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("marker.txt"), b"old").unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("marker.txt"), b"new").unwrap();

        migrate_data_dir(&old_dir, &new_dir);

        assert!(old_dir.exists(), "old dir must be left alone, not merged or deleted");
        assert_eq!(std::fs::read_to_string(new_dir.join("marker.txt")).unwrap(), "new");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn migrate_data_dir_does_nothing_when_no_old_folder_exists() {
        let tmp = std::env::temp_dir().join(format!("mig-test-fresh-{}", std::process::id()));
        let old_dir = tmp.join("MultiAIAgentsPanel");
        let new_dir = tmp.join("Multi-AI Agents Panel");

        migrate_data_dir(&old_dir, &new_dir);

        assert!(!new_dir.exists());
    }
}
