//! Tauri commands backing the AI Control Center page: API Key management,
//! cloud model pickers, and local Ollama model management.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::agent_manager::curated_models::{self, CuratedModel};
use crate::agent_manager::providers::ollama;
use crate::key_vault;
use crate::storage::{ProviderKey, Storage, UsageSummary};

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
