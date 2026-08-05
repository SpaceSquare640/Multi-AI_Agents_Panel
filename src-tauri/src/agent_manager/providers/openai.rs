//! OpenAI adapter — the Chat Completions API.
//! https://platform.openai.com/docs/api-reference/chat
//!
//! Same request/response shape as `openrouter` (OpenRouter is itself
//! OpenAI-compatible) — only the base URL differs. Kept as a separate
//! module rather than a shared function with a URL parameter because the
//! two providers are expected to diverge over time (e.g. OpenAI-specific
//! parameters), and a thin duplicate is easier to evolve independently
//! than an abstraction built for a divergence that hasn't happened yet.

use serde_json::{json, Value};

use super::{ChatMessage, ProviderError};

const API_URL: &str = "https://api.openai.com/v1/chat/completions";

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
        // OpenAI reports both `error.type` (e.g. "invalid_request_error")
        // and `error.code` (e.g. "invalid_api_key", "model_not_found") —
        // check both, no numeric HTTP code in the body itself.
        let error_type = error.get("type").and_then(Value::as_str).unwrap_or("");
        let error_code_field = error.get("code").and_then(Value::as_str).unwrap_or("");
        let signal = format!("{error_type} {error_code_field}");
        let error_code = super::classify_cloud_api_error(&signal, None);
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
        let body = build_request("gpt-4.1-mini", &messages);
        assert_eq!(body["model"], "gpt-4.1-mini");
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
            "error": { "message": "invalid api key", "code": "invalid_api_key" }
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
}

/// Live smoke test against the real OpenAI API. Not run by default — it
/// needs a real API key and network access, neither of which CI has. Run
/// manually with:
///   OPENAI_TEST_KEY=... cargo test --manifest-path src-tauri/Cargo.toml -- --ignored openai::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn send_reaches_a_real_model() {
        let api_key = std::env::var("OPENAI_TEST_KEY").expect("set OPENAI_TEST_KEY to run this test");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly one word: pong".to_string(),
        }];
        let reply = send(&api_key, "gpt-4.1-mini", &messages).expect("live call to OpenAI failed");
        assert!(!reply.trim().is_empty());
    }
}
