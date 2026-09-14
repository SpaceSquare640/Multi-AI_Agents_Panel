//! Tool-calling adapter for the OpenAI-compatible chat-completions shape,
//! shared by the OpenAI and OpenRouter providers.
//!
//! One module rather than two because, unlike the plain-chat adapters
//! (`openai`/`openrouter`, kept separate so they can diverge), there is
//! nothing here to diverge: tool calling *is* the OpenAI protocol, and
//! OpenRouter passes it through unchanged. Only the endpoint URL and the
//! error-body dialect differ, and both are parameters rather than
//! duplicated code.
//!
//! How this differs from Anthropic, which is the whole reason
//! `agent_manager::function_calling` needed a dialect seam at all:
//!
//! | | Anthropic | OpenAI-compatible |
//! |---|---|---|
//! | tool list | `input_schema` on the tool | `function.parameters` |
//! | model's call | `tool_use` block in `content` | `tool_calls` on the message |
//! | arguments | a JSON object | a JSON **string** to be parsed |
//! | result turn | `role: "user"` + `tool_result` block | `role: "tool"` message |
//! | failure flag | `is_error: true` | *none* — folded into the text |

use serde_json::{json, Value};

use super::{ProviderError, ToolTurn};

pub const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// One entry of the `tools` array. The schema is generic for the same
/// reason it is on the Anthropic side — Skills carry no per-tool JSON
/// Schema today (see `function_calling`'s module docs). `properties` is
/// spelled out rather than left off because some OpenAI-compatible
/// gateways reject a parameters object without it.
pub fn tool_spec(name: &str, description: &str) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {"type": "object", "properties": {}},
        },
    })
}

/// The assistant turn that records a tool call. `arguments` must be a
/// JSON *string*, not an object — this is the shape the API emitted and
/// the shape it expects echoed back, and sending the object form makes
/// the follow-up request fail validation.
pub fn assistant_tool_call(id: &str, name: &str, input: &Value) -> Value {
    json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": [{
            "id": id,
            "type": "function",
            "function": { "name": name, "arguments": input.to_string() },
        }],
    })
}

/// The turn carrying a tool's result back to the model.
///
/// There is no `is_error` field in this protocol, so a failed tool run
/// has to reach the model as text or not at all. The `[tool error]`
/// prefix is that text: without it a serialised `{"error": "..."}` object
/// is indistinguishable from a skill that legitimately returned a field
/// called `error`, and the model has no other signal to tell the two
/// apart. Anthropic's `tool_result` block carries the flag properly.
pub fn tool_result(id: &str, output: &Value, is_error: bool) -> Value {
    let content = if is_error { format!("[tool error] {output}") } else { output.to_string() };
    json!({ "role": "tool", "tool_call_id": id, "content": content })
}

/// Builds a chat-completions request carrying raw, already-shaped message
/// JSON plus a `tools` array. The system prompt is an ordinary
/// `role: "system"` message here, prepended to the conversation — unlike
/// Anthropic, where it is a top-level field.
pub fn build_tooled_request(model: &str, system: Option<&str>, raw_messages: &[Value], tools: &[Value]) -> Value {
    let mut messages: Vec<Value> = Vec::with_capacity(raw_messages.len() + 1);
    if let Some(system) = system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.extend(raw_messages.iter().cloned());

    json!({ "model": model, "messages": messages, "tools": tools })
}

/// Extracts either the first tool call or the message text from a
/// chat-completions response built with `tools` on the request.
///
/// The tool call is checked first for the same reason as on the Anthropic
/// side: a turn may carry both some commentary text and the call, and it
/// is the call the loop has to act on.
pub fn parse_tooled_response(body: &Value) -> Result<ToolTurn, ProviderError> {
    if let Some(error) = body.get("error") {
        return Err(classify_error(error));
    }

    let message = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| ProviderError::Api { error_code: "E2000", message: "response had no message".to_string() })?;

    if let Some(call) = message.get("tool_calls").and_then(Value::as_array).and_then(|calls| calls.first()) {
        let id = call.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        let function = call.get("function").unwrap_or(&Value::Null);
        let name = function.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let input = parse_arguments(function.get("arguments"))?;
        return Ok(ToolTurn::ToolUse { id, name, input });
    }

    if let Some(text) = message.get("content").and_then(Value::as_str) {
        return Ok(ToolTurn::Text(text.to_string()));
    }

    Err(ProviderError::Api { error_code: "E2000", message: "response had no message content or tool call".to_string() })
}

/// Turns the `arguments` JSON string into the object the tool loop passes
/// to `invoke_skill`.
///
/// An absent or blank `arguments` means "no arguments" and becomes `{}` —
/// models routinely emit `""` for a zero-argument call, and treating that
/// as malformed would fail every such call. Anything else that will not
/// parse is a genuine protocol violation and is reported rather than
/// guessed at: inventing an empty payload would run the skill with the
/// wrong input instead of saying the model's call was unusable.
fn parse_arguments(arguments: Option<&Value>) -> Result<Value, ProviderError> {
    let raw = match arguments {
        None | Some(Value::Null) => return Ok(json!({})),
        // Some gateways relay the object form despite the spec; accept it.
        Some(value @ Value::Object(_)) => return Ok(value.clone()),
        Some(Value::String(raw)) => raw,
        Some(other) => {
            return Err(ProviderError::Api {
                error_code: "E2000",
                message: format!("tool call arguments were {other}, not a JSON string"),
            })
        }
    };

    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(raw).map_err(|e| ProviderError::Api {
        error_code: "E2000",
        message: format!("tool call arguments were not valid JSON: {e}"),
    })
}

/// Maps a chat-completions error body onto an Error Code Registry code.
/// Handles both dialects in one place: OpenAI reports `type`/`code` as
/// strings, OpenRouter reports an HTTP-style numeric `code`.
fn classify_error(error: &Value) -> ProviderError {
    let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
    let numeric_code = error.get("code").and_then(Value::as_i64);
    let error_type = error.get("type").and_then(Value::as_str).unwrap_or("");
    let string_code = error.get("code").and_then(Value::as_str).unwrap_or("");
    let signal = format!("{error_type} {string_code} {message}");
    let error_code = super::classify_cloud_api_error(&signal, numeric_code);
    ProviderError::Api { error_code, message: message.to_string() }
}

pub fn send_tooled(
    api_url: &str,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    raw_messages: &[Value],
    tools: &[Value],
) -> Result<ToolTurn, ProviderError> {
    let client = crate::http::cloud_inference();
    let response = client
        .post(api_url)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&build_tooled_request(model, system, raw_messages, tools))
        .send()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E2003", message: e.to_string() })?;

    parse_tooled_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_spec_wraps_the_schema_in_a_function_object() {
        let spec = tool_spec("greeter", "greets people");
        assert_eq!(spec["type"], "function");
        assert_eq!(spec["function"]["name"], "greeter");
        assert_eq!(spec["function"]["description"], "greets people");
        assert_eq!(spec["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn assistant_tool_call_serialises_arguments_as_a_string_not_an_object() {
        let turn = assistant_tool_call("call_1", "greeter", &json!({"name": "Ada"}));
        let arguments = turn["tool_calls"][0]["function"]["arguments"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(arguments).unwrap(), json!({"name": "Ada"}));
    }

    #[test]
    fn tool_result_marks_a_failure_in_the_text_since_the_protocol_has_no_error_flag() {
        let ok = tool_result("call_1", &json!({"winners": ["A"]}), false);
        assert_eq!(ok["role"], "tool");
        assert_eq!(ok["tool_call_id"], "call_1");
        assert!(!ok["content"].as_str().unwrap().contains("[tool error]"));

        let failed = tool_result("call_1", &json!({"error": "not authorized"}), true);
        let content = failed["content"].as_str().unwrap();
        assert!(content.starts_with("[tool error]"));
        assert!(content.contains("not authorized"));
    }

    #[test]
    fn build_tooled_request_prepends_the_system_prompt_as_a_message() {
        let raw_messages = vec![json!({"role": "user", "content": "hi"})];
        let tools = vec![tool_spec("greeter", "greets people")];
        let body = build_tooled_request("gpt-4.1-mini", Some("be terse"), &raw_messages, &tools);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"][0], json!({"role": "system", "content": "be terse"}));
        assert_eq!(body["messages"][1], raw_messages[0]);
        assert_eq!(body["tools"], json!(tools));
    }

    #[test]
    fn build_tooled_request_omits_the_system_message_when_there_is_none() {
        let raw_messages = vec![json!({"role": "user", "content": "hi"})];
        let body = build_tooled_request("gpt-4.1-mini", None, &raw_messages, &[]);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn parse_tooled_response_extracts_a_tool_call_and_parses_its_string_arguments() {
        let body = json!({
            "choices": [{"message": {
                "role": "assistant",
                "content": "Let me check that for you.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "raffle_winner_picker", "arguments": "{\"entries\": [\"A\", \"B\"]}"},
                }],
            }}]
        });
        assert_eq!(
            parse_tooled_response(&body).unwrap(),
            ToolTurn::ToolUse {
                id: "call_1".to_string(),
                name: "raffle_winner_picker".to_string(),
                input: json!({"entries": ["A", "B"]}),
            }
        );
    }

    #[test]
    fn parse_tooled_response_treats_blank_arguments_as_a_call_with_no_arguments() {
        let body = json!({
            "choices": [{"message": {
                "tool_calls": [{"id": "call_1", "function": {"name": "now", "arguments": ""}}],
            }}]
        });
        assert_eq!(
            parse_tooled_response(&body).unwrap(),
            ToolTurn::ToolUse { id: "call_1".to_string(), name: "now".to_string(), input: json!({}) }
        );
    }

    #[test]
    fn parse_tooled_response_reports_malformed_arguments_rather_than_guessing_an_empty_payload() {
        let body = json!({
            "choices": [{"message": {
                "tool_calls": [{"id": "call_1", "function": {"name": "greeter", "arguments": "{not json"}}],
            }}]
        });
        let err = parse_tooled_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2000", ref message } if message.contains("not valid JSON")));
    }

    #[test]
    fn parse_tooled_response_falls_back_to_text_when_there_is_no_tool_call() {
        let body = json!({"choices": [{"message": {"role": "assistant", "content": "hi there"}}]});
        assert_eq!(parse_tooled_response(&body).unwrap(), ToolTurn::Text("hi there".to_string()));
    }

    #[test]
    fn parse_tooled_response_classifies_the_openai_string_error_dialect() {
        let body = json!({"error": {"message": "invalid api key", "code": "invalid_api_key"}});
        let err = parse_tooled_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2001", .. }));
    }

    #[test]
    fn parse_tooled_response_classifies_the_openrouter_numeric_error_dialect() {
        let body = json!({"error": {"message": "rate limited", "code": 429}});
        let err = parse_tooled_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2002", .. }));
    }
}
