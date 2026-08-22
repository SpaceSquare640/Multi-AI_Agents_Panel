//! Anthropic Messages API adapter — the first cloud provider, per the
//! agreed dev order (Storage → Key Vault → Agent Manager/one provider → ...).
//! <https://docs.anthropic.com/en/api/messages>

use serde_json::{json, Value};

use super::{ChatMessage, ProviderError};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Builds the JSON request body for the Messages API. Pulled out as a pure
/// function so the request shape can be unit-tested without any network.
///
/// Anthropic's API is the odd one out among our providers: the system
/// prompt is a top-level `system` field, not a `role: "system"` entry in
/// `messages` (which must strictly alternate user/assistant). So any
/// `ChatMessage`s with `role == "system"` are pulled out and joined into
/// that field instead of being passed through — unlike the OpenRouter/
/// Ollama adapters, which accept `role: "system"` inline.
pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    let system_prompt: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect();

    let mut body = json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect::<Vec<_>>(),
    });

    if !system_prompt.is_empty() {
        body["system"] = json!(system_prompt.join("\n\n"));
    }

    body
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
        // Anthropic reports a machine-readable `error.type` (e.g.
        // "authentication_error", "rate_limit_error") — no numeric HTTP
        // code in the body itself.
        let error_type = error.get("type").and_then(Value::as_str).unwrap_or("");
        let error_code = super::classify_cloud_api_error(error_type, None);
        return Err(ProviderError::Api { error_code, message: message.to_string() });
    }

    body.get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api {
            error_code: "E2000",
            message: "response had no text content block".to_string(),
        })
}

/// One turn of a tool-calling conversation: either the model answered in
/// plain text, or it wants a tool run before it can continue. Mirrors
/// the two shapes Anthropic's `content` array can hold when `tools` is
/// passed on the request — see `parse_tooled_response`.
#[derive(Debug, Clone, PartialEq)]
pub enum AnthropicReply {
    Text(String),
    ToolUse { id: String, name: String, input: Value },
}

/// Builds a Messages API request carrying raw, already-shaped message
/// JSON (as opposed to `build_request`'s plain `ChatMessage` list) plus a
/// `tools` array, for the multi-turn tool-calling loop in
/// `agent_manager::function_calling`. Raw messages are needed here
/// because a tool-calling turn's `content` is a block array (`tool_use`/
/// `tool_result`), not the single string `ChatMessage` represents — the
/// plain-chat path (`build_request`/`send`) is untouched by this.
pub fn build_tooled_request(model: &str, system: Option<&str>, raw_messages: &[Value], tools: &[Value]) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": raw_messages,
        "tools": tools,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    body
}

/// Extracts either the first `tool_use` block (checked first — a
/// tool-calling turn's `content` array can include reasoning text
/// alongside it, but the tool call is what the loop needs to act on) or,
/// failing that, the first `text` block, from a Messages API response
/// built with `tools` on the request.
pub fn parse_tooled_response(body: &Value) -> Result<AnthropicReply, ProviderError> {
    if let Some(error) = body.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
        let error_type = error.get("type").and_then(Value::as_str).unwrap_or("");
        let error_code = super::classify_cloud_api_error(error_type, None);
        return Err(ProviderError::Api { error_code, message: message.to_string() });
    }

    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Api { error_code: "E2000", message: "response had no content blocks".to_string() })?;

    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
            let id = block.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let name = block.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let input = block.get("input").cloned().unwrap_or(json!({}));
            return Ok(AnthropicReply::ToolUse { id, name, input });
        }
    }
    for block in blocks {
        if let Some(text) = block.get("text").and_then(Value::as_str) {
            return Ok(AnthropicReply::Text(text.to_string()));
        }
    }
    Err(ProviderError::Api { error_code: "E2000", message: "response had no text or tool_use content block".to_string() })
}

pub fn send_tooled(
    api_key: &str,
    model: &str,
    system: Option<&str>,
    raw_messages: &[Value],
    tools: &[Value],
) -> Result<AnthropicReply, ProviderError> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&build_tooled_request(model, system, raw_messages, tools))
        .send()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    parse_tooled_response(&body)
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
        let body = build_request("claude-sonnet", &messages);
        assert_eq!(body["model"], "claude-sonnet");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn build_request_pulls_system_messages_into_the_top_level_field() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are the Product Lead.".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
            },
        ];
        let body = build_request("claude-sonnet", &messages);
        assert_eq!(body["system"], "You are the Product Lead.");
        // Only the non-system message remains in `messages`.
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn build_request_omits_system_field_when_there_is_none() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let body = build_request("claude-sonnet", &messages);
        assert!(body.get("system").is_none());
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
        assert!(matches!(err, ProviderError::Api { ref message, .. } if message == "invalid x-api-key"));
        // "authentication_error" should classify as E2001 (invalid/missing key).
        assert!(matches!(err, ProviderError::Api { error_code: "E2001", .. }));
    }

    #[test]
    fn parse_response_rejects_missing_content() {
        let body = json!({});
        assert!(parse_response(&body).is_err());
    }

    #[test]
    fn build_tooled_request_includes_tools_and_raw_messages_verbatim() {
        let raw_messages = vec![json!({"role": "user", "content": "hi"})];
        let tools = vec![json!({"name": "raffle_winner_picker", "description": "picks winners", "input_schema": {"type": "object"}})];
        let body = build_tooled_request("claude-sonnet", Some("be terse"), &raw_messages, &tools);
        assert_eq!(body["system"], "be terse");
        assert_eq!(body["messages"], json!(raw_messages));
        assert_eq!(body["tools"], json!(tools));
    }

    #[test]
    fn parse_tooled_response_extracts_a_tool_use_block_over_a_text_block() {
        let body = json!({
            "content": [
                {"type": "text", "text": "Let me check that for you."},
                {"type": "tool_use", "id": "toolu_1", "name": "raffle_winner_picker", "input": {"entries": ["A", "B"]}},
            ]
        });
        let reply = parse_tooled_response(&body).unwrap();
        assert_eq!(
            reply,
            AnthropicReply::ToolUse {
                id: "toolu_1".to_string(),
                name: "raffle_winner_picker".to_string(),
                input: json!({"entries": ["A", "B"]}),
            }
        );
    }

    #[test]
    fn parse_tooled_response_falls_back_to_text_when_there_is_no_tool_use_block() {
        let body = json!({"content": [{"type": "text", "text": "hi there"}]});
        assert_eq!(parse_tooled_response(&body).unwrap(), AnthropicReply::Text("hi there".to_string()));
    }

    #[test]
    fn parse_tooled_response_surfaces_api_errors_the_same_as_the_plain_path() {
        let body = json!({"error": {"type": "rate_limit_error", "message": "slow down"}});
        let err = parse_tooled_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2002", ref message } if message == "slow down"));
    }
}
