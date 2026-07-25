//! Anthropic Messages API adapter — the first cloud provider, per the
//! agreed dev order (Storage → Key Vault → Agent Manager/one provider → ...).
//! https://docs.anthropic.com/en/api/messages

use serde_json::{json, Value};

use super::{ChatMessage, ProviderError};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Builds the JSON request body for the Messages API. Pulled out as a pure
/// function so the request shape can be unit-tested without any network.
pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect::<Vec<_>>(),
    })
}

/// Extracts the assistant's reply text from a Messages API response body.
/// Pulled out as a pure function so response parsing can be unit-tested
/// against fixed JSON without any network.
pub fn parse_response(body: &Value) -> Result<String, ProviderError> {
    if let Some(error) = body.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(ProviderError::Api(message.to_string()));
    }

    body.get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api("response had no text content block".to_string()))
}

pub fn send(api_key: &str, model: &str, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&build_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network(e.to_string()))?;

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
        let body = build_request("claude-sonnet", &messages);
        assert_eq!(body["model"], "claude-sonnet");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn parse_response_extracts_text() {
        let body = json!({
            "content": [{ "type": "text", "text": "hi there" }]
        });
        assert_eq!(parse_response(&body).unwrap(), "hi there");
    }

    #[test]
    fn parse_response_surfaces_api_error() {
        let body = json!({
            "error": { "type": "authentication_error", "message": "invalid x-api-key" }
        });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api(msg) if msg == "invalid x-api-key"));
    }

    #[test]
    fn parse_response_rejects_missing_content() {
        let body = json!({});
        assert!(parse_response(&body).is_err());
    }
}
