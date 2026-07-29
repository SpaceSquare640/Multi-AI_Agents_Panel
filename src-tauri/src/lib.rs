mod agent_manager;
mod commands;
mod fallback;
mod file_access;
mod key_vault;
mod orchestrator;
mod session_manager;
mod skill_manager;
mod storage;
mod usage_tracker;

use storage::Storage;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("data.sqlite3");
            let storage = Storage::open(&db_path)
                .map_err(|e| format!("failed to open storage at {:?}: {e}", db_path))?;
            app.manage(storage);
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
            commands::list_sessions,
            commands::create_independent_session,
            commands::list_messages,
            commands::get_session_agent_id,
            commands::send_chat_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
