//! OpenRouter adapter — OpenAI-compatible chat completions API.
//! <https://openrouter.ai/docs/api-reference/chat-completion>

use serde_json::{json, Value};

use super::{ChatMessage, ProviderError, TokenUsage};

const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Builds the JSON request body. Pulled out as a pure function so the
/// request shape can be unit-tested without any network.
pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    json!({
        "model": model,
        "messages": messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect::<Vec<_>>(),
    })
}

/// Extracts the assistant's reply text from a chat-completions response
/// body. Pulled out as a pure function so response parsing can be
/// unit-tested against fixed JSON without any network.
pub fn parse_response(body: &Value) -> Result<String, ProviderError> {
    if let Some(error) = body.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        // OpenRouter reports an HTTP-style numeric `error.code` (e.g. 401,
        // 429) — the most reliable signal available, checked before
        // falling back to the message text.
        let numeric_code = error.get("code").and_then(Value::as_i64);
        let error_code = super::classify_cloud_api_error(message, numeric_code);
        return Err(ProviderError::Api { error_code, message: message.to_string() });
    }

    body.get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api {
            error_code: "E2000",
            message: "response had no message content".to_string(),
        })
}

/// Extracts token usage from a chat-completions response body's
/// OpenAI-compatible `usage` object, if present. `None` (not zero) when
/// the field is missing — some OpenRouter-routed models don't report
/// usage — so a caller can tell "unknown" from "reported zero" and skip
/// cost estimation rather than silently claiming a free call.
#[allow(dead_code)] // staged — see agent_manager::cost's module docs
fn parse_usage(body: &Value) -> Option<TokenUsage> {
    let usage = body.get("usage")?;
    Some(TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
    })
}

/// Same call as `send`, but also returns whatever token usage the
/// response reported — used by `dispatch_one`'s OpenRouter branch so
/// `usage_log` can record an estimated cost (see `agent_manager::cost`).
/// A separate function rather than changing `send`'s signature: the
/// plain-text path (`send`, used by tool-calling/DAG/memory contexts
/// that only need the reply) is unaffected, and every existing test
/// against `send`/`parse_response` still holds.
#[allow(dead_code)] // staged — see agent_manager::cost's module docs
pub fn send_with_usage(api_key: &str, model: &str, messages: &[ChatMessage]) -> Result<(String, Option<TokenUsage>), ProviderError> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(API_URL)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&build_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    let text = parse_response(&body)?;
    Ok((text, parse_usage(&body)))
}

pub fn send(api_key: &str, model: &str, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(API_URL)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&build_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    parse_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_shapes_messages() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];
        let body = build_request("inclusionai/ling-3.0-flash:free", &messages);
        assert_eq!(body["model"], "inclusionai/ling-3.0-flash:free");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn parse_response_extracts_text() {
        let body = json!({
            "choices": [{ "message": { "role": "assistant", "content": "hi there" } }]
        });
        assert_eq!(parse_response(&body).unwrap(), "hi there");
    }

    #[test]
    fn parse_response_surfaces_api_error() {
        let body = json!({
            "error": { "message": "invalid api key", "code": 401 }
        });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { ref message, .. } if message == "invalid api key"));
        assert!(matches!(err, ProviderError::Api { error_code: "E2001", .. }));
    }

    #[test]
    fn parse_response_rejects_missing_content() {
        let body = json!({});
        assert!(parse_response(&body).is_err());
    }

    #[test]
    fn parse_usage_reads_prompt_and_completion_tokens() {
        let body = json!({"usage": {"prompt_tokens": 12, "completion_tokens": 34, "total_tokens": 46}});
        assert_eq!(parse_usage(&body), Some(TokenUsage { prompt_tokens: 12, completion_tokens: 34 }));
    }

    #[test]
    fn parse_usage_is_none_rather_than_zero_when_the_field_is_absent() {
        let body = json!({"choices": []});
        assert_eq!(parse_usage(&body), None);
    }
}

/// Live smoke tests against the real OpenRouter API. Not run by default —
/// they need a real (free-tier) API key and network access, neither of
/// which CI has. Run manually with:
///   OPENROUTER_TEST_KEY=... cargo test --manifest-path src-tauri/Cargo.toml -- --ignored openrouter::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn send_reaches_a_free_model() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY")
            .expect("set OPENROUTER_TEST_KEY to run this test");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly one word: pong".to_string(),
        }];
        let reply = send(&api_key, "inclusionai/ling-3.0-flash:free", &messages)
            .expect("live call to OpenRouter failed");
        assert!(!reply.trim().is_empty());
    }
}
