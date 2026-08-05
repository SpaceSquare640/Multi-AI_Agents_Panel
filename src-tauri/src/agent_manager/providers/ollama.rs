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
        // Ollama's error is a plain string, not a structured type/code —
        // "model '...' not found" is the one common, recognizable case
        // (E1002, matching the Error Code Registry's "本地模型檔案缺失").
        // Anything else falls to E1000, the registry's documented E1xxx
        // catch-all, rather than guessing at a more specific code.
        let error_code = if err.to_lowercase().contains("not found") { "E1002" } else { "E1000" };
        return Err(ProviderError::Api { error_code, message: err.to_string() });
    }
    body.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api {
            error_code: "E1000",
            message: "response had no message content".to_string(),
        })
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
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach Ollama: {e}"),
        })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E1001", message: e.to_string() })?;
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
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach Ollama: {e}"),
        })?;
    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E1001", message: e.to_string() })?;
    Ok(parse_tags_response(&body))
}

/// One line of Ollama's `/api/pull` streaming NDJSON response — see
/// `parse_pull_progress_line`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    /// Ollama's own status text, e.g. "pulling manifest",
    /// "downloading sha256:...", "verifying sha256 digest", "success".
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    /// 0–100, only present when this line reported both `completed` and
    /// a non-zero `total` — most status lines (e.g. "pulling manifest")
    /// report neither, so this is `None` far more often than not.
    pub percent: Option<f64>,
}

/// Parses one line of Ollama's `/api/pull?stream=true` NDJSON body.
/// Returns `None` for blank lines or lines that don't parse as the
/// expected shape, rather than failing the whole stream over one odd
/// line — pure logic, testable without a real download.
pub fn parse_pull_progress_line(line: &str) -> Option<PullProgress> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    let status = value.get("status")?.as_str()?.to_string();
    let completed = value.get("completed").and_then(Value::as_u64);
    let total = value.get("total").and_then(Value::as_u64);
    let percent = match (completed, total) {
        (Some(c), Some(t)) if t > 0 => Some((c as f64 / t as f64) * 100.0),
        _ => None,
    };
    Some(PullProgress { status, completed, total, percent })
}

/// Pulls (installs) a model, calling `on_progress` once per NDJSON line
/// Ollama streams back — real download progress, not a blocking call
/// with no feedback until it's done. Reads the response body
/// incrementally line-by-line (`BufReader` over the still-open HTTP
/// connection) rather than buffering the whole body first, so
/// `on_progress` fires as bytes actually arrive.
pub fn pull_model_with_progress(
    name: &str,
    mut on_progress: impl FnMut(PullProgress),
) -> Result<(), ProviderError> {
    let response = client()
        .post(format!("{BASE_URL}/api/pull"))
        .json(&json!({ "name": name, "stream": true }))
        .send()
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach Ollama: {e}"),
        })?;

    if !response.status().is_success() {
        return Err(ProviderError::Api {
            error_code: "E1000",
            message: format!("pull failed with status {}", response.status()),
        });
    }

    let reader = std::io::BufReader::new(response);
    let mut last_status = String::new();
    for line in std::io::BufRead::lines(reader) {
        let line = line.map_err(|e| ProviderError::Network { error_code: "E1001", message: e.to_string() })?;
        if let Some(progress) = parse_pull_progress_line(&line) {
            last_status = progress.status.clone();
            on_progress(progress);
        }
    }
    if last_status.to_lowercase().contains("error") {
        return Err(ProviderError::Api { error_code: "E1000", message: format!("pull failed: {last_status}") });
    }
    Ok(())
}

pub fn delete_model(name: &str) -> Result<(), ProviderError> {
    let response = client()
        .delete(format!("{BASE_URL}/api/delete"))
        .json(&json!({ "name": name }))
        .send()
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach Ollama: {e}"),
        })?;

    if !response.status().is_success() {
        return Err(ProviderError::Api {
            error_code: "E1000",
            message: format!("delete failed with status {}", response.status()),
        });
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
        assert!(matches!(err, ProviderError::Api { ref message, .. } if message == "model 'nope' not found"));
        // "not found" should classify as E1002 (local model file missing).
        assert!(matches!(err, ProviderError::Api { error_code: "E1002", .. }));
    }

    #[test]
    fn parse_chat_response_falls_back_to_the_e1000_catch_all_for_unrecognized_errors() {
        let body = json!({ "error": "something went wrong internally" });
        let err = parse_chat_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E1000", .. }));
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

    #[test]
    fn parse_pull_progress_line_computes_percent_when_both_fields_present() {
        let line = r#"{"status":"downloading sha256:abc","total":1000,"completed":250}"#;
        let progress = parse_pull_progress_line(line).unwrap();
        assert_eq!(progress.status, "downloading sha256:abc");
        assert_eq!(progress.completed, Some(250));
        assert_eq!(progress.total, Some(1000));
        assert_eq!(progress.percent, Some(25.0));
    }

    #[test]
    fn parse_pull_progress_line_has_no_percent_when_total_or_completed_missing() {
        let line = r#"{"status":"pulling manifest"}"#;
        let progress = parse_pull_progress_line(line).unwrap();
        assert_eq!(progress.status, "pulling manifest");
        assert_eq!(progress.percent, None);
    }

    #[test]
    fn parse_pull_progress_line_returns_none_for_a_blank_line() {
        assert!(parse_pull_progress_line("").is_none());
        assert!(parse_pull_progress_line("   ").is_none());
    }

    #[test]
    fn parse_pull_progress_line_returns_none_for_malformed_json() {
        assert!(parse_pull_progress_line("not json at all").is_none());
    }

    #[test]
    fn parse_pull_progress_line_treats_a_zero_total_as_no_percent_to_avoid_dividing_by_zero() {
        let line = r#"{"status":"downloading","total":0,"completed":0}"#;
        let progress = parse_pull_progress_line(line).unwrap();
        assert_eq!(progress.percent, None);
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
