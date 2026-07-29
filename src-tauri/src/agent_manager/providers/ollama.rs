//! Ollama adapter — both a chat provider (local models, no API key) and the
//! local-model management operations (list/pull/delete) used by the AI
//! Control Center's "Local AI Model" section.
//! https://github.com/ollama/ollama/blob/main/docs/api.md

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{ChatMessage, ProviderError};

const BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    /// Bytes on disk, if Ollama reported one.
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::new()
}

/// Builds the JSON request body for `/api/chat`. Pure function, unit-testable.
pub fn build_chat_request(model: &str, messages: &[ChatMessage]) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect::<Vec<_>>(),
    })
}

/// Extracts the assistant's reply text from a `/api/chat` response body.
/// Pure function, unit-testable.
pub fn parse_chat_response(body: &Value) -> Result<String, ProviderError> {
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        return Err(ProviderError::Api(err.to_string()));
    }
    body.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api("response had no message content".to_string()))
}

/// Extracts the installed-model list from an `/api/tags` response body.
/// Pure function, unit-testable.
pub fn parse_tags_response(body: &Value) -> Vec<OllamaModel> {
    body.get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| {
                    let name = m.get("name").and_then(Value::as_str)?.to_string();
                    Some(OllamaModel {
                        name,
                        size: m.get("size").and_then(Value::as_u64),
                        modified_at: m
                            .get("modified_at")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn send(model: &str, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    let response = client()
        .post(format!("{BASE_URL}/api/chat"))
        .json(&build_chat_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network(format!("could not reach Ollama: {e}")))?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    parse_chat_response(&body)
}

/// Cheap reachability check — used by the UI to show "Ollama not running"
/// instead of a confusing network error.
pub fn is_running() -> bool {
    client()
        .get(format!("{BASE_URL}/api/tags"))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub fn list_installed() -> Result<Vec<OllamaModel>, ProviderError> {
    let response = client()
        .get(format!("{BASE_URL}/api/tags"))
        .send()
        .map_err(|e| ProviderError::Network(format!("could not reach Ollama: {e}")))?;
    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    Ok(parse_tags_response(&body))
}

/// Pulls (installs) a model. This blocks until the download finishes —
/// there's no progress percentage surfaced yet (see Backlog: streaming
/// pull progress is a follow-up, not this pass).
pub fn pull_model(name: &str) -> Result<(), ProviderError> {
    let response = client()
        .post(format!("{BASE_URL}/api/pull"))
        .json(&json!({ "name": name, "stream": false }))
        .send()
        .map_err(|e| ProviderError::Network(format!("could not reach Ollama: {e}")))?;

    if !response.status().is_success() {
        return Err(ProviderError::Api(format!(
            "pull failed with status {}",
            response.status()
        )));
    }
    Ok(())
}

pub fn delete_model(name: &str) -> Result<(), ProviderError> {
    let response = client()
        .delete(format!("{BASE_URL}/api/delete"))
        .json(&json!({ "name": name }))
        .send()
        .map_err(|e| ProviderError::Network(format!("could not reach Ollama: {e}")))?;

    if !response.status().is_success() {
        return Err(ProviderError::Api(format!(
            "delete failed with status {}",
            response.status()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_chat_request_shapes_messages() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];
        let body = build_chat_request("llama3.1:8b", &messages);
        assert_eq!(body["model"], "llama3.1:8b");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn parse_chat_response_extracts_text() {
        let body = json!({ "message": { "role": "assistant", "content": "hi there" }, "done": true });
        assert_eq!(parse_chat_response(&body).unwrap(), "hi there");
    }

    #[test]
    fn parse_chat_response_surfaces_error() {
        let body = json!({ "error": "model 'nope' not found" });
        let err = parse_chat_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api(msg) if msg == "model 'nope' not found"));
    }

    #[test]
    fn parse_tags_response_lists_models() {
        let body = json!({
            "models": [
                { "name": "llama3.1:8b", "size": 4_700_000_000u64, "modified_at": "2026-01-01T00:00:00Z" },
                { "name": "qwen2.5-coder:1.5b" }
            ]
        });
        let models = parse_tags_response(&body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3.1:8b");
        assert_eq!(models[0].size, Some(4_700_000_000));
        assert_eq!(models[1].size, None);
    }

    #[test]
    fn parse_tags_response_handles_empty_body() {
        assert!(parse_tags_response(&json!({})).is_empty());
    }
}

/// Live smoke tests against a real local Ollama install. Not run by
/// default — needs Ollama actually running on localhost. Run manually with:
///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored ollama::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn lists_installed_models_if_ollama_is_running() {
        assert!(is_running(), "Ollama does not appear to be running on localhost:11434");
        let models = list_installed().unwrap();
        println!("installed models: {models:?}");
    }
}
