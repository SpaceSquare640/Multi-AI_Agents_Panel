//! OmniRoute adapter — a local, self-hosted AI gateway
//! (<https://github.com/diegosouzapw/OmniRoute>) that aggregates many
//! providers' free/paid tiers behind one OpenAI-compatible endpoint on
//! `localhost:20128` by default. Same request/response shape as
//! `openai`/`openrouter`/`colibri`, treated as a *local* provider like
//! `ollama`/`colibri` (no Key Vault entry, no cloud fallback rotation)
//! — the user runs their own `omniroute` process (npm/Docker/Electron),
//! there's no account this app manages.
//!
//! Per OmniRoute's own README, `model: "auto"` (and its keyless free
//! backends, e.g. `oc/...`) works with **no API key at all** — a fresh
//! install answers unauthenticated. OmniRoute's dashboard can also
//! generate a key that unlocks more providers; this adapter sends one
//! only if the *app's own* process has `OMNIROUTE_API_KEY` set, same
//! opt-in-via-env pattern as `colibri.rs` — never stored in Key Vault,
//! since it authenticates to a local process on the same machine, not a
//! cloud account.

use serde_json::Value;

use super::{ChatMessage, ProviderError};

const API_URL: &str = "http://localhost:20128/v1/chat/completions";

/// Builds the JSON request body — identical shape to `openai::build_request`
/// (OmniRoute's gateway is OpenAI-compatible), kept as its own pure
/// function so this adapter can evolve independently.
pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    super::openai::build_request(model, messages)
}

/// Extracts the assistant's reply, or classifies an error. OmniRoute is
/// a local process the user runs themselves, so errors are coded like
/// Ollama's/colibrì's (E1xxx), not the cloud E2xxx range.
pub fn parse_response(body: &Value) -> Result<String, ProviderError> {
    if let Some(error) = body.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string();
        let error_code = if message.to_lowercase().contains("not found") { "E1002" } else { "E1000" };
        return Err(ProviderError::Api { error_code, message });
    }

    body.get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Api {
            error_code: "E1000",
            message: "response had no message content".to_string(),
        })
}

pub fn send(model: &str, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.post(API_URL).header("content-type", "application/json");
    if let Ok(api_key) = std::env::var("OMNIROUTE_API_KEY") {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .json(&build_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach OmniRoute (is it running on localhost:20128?): {e}"),
        })?;

    let body: Value = response
        .json()
        .map_err(|e| ProviderError::Network { error_code: "E1001", message: e.to_string() })?;
    parse_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_shapes_messages() {
        let messages = vec![ChatMessage { role: "user".to_string(), content: "hello".to_string() }];
        let body = build_request("auto", &messages);
        assert_eq!(body["model"], "auto");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn parse_response_extracts_text() {
        let body = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "hi there" } }]
        });
        assert_eq!(parse_response(&body).unwrap(), "hi there");
    }

    #[test]
    fn parse_response_classifies_model_not_found_as_e1002() {
        let body = serde_json::json!({ "error": { "message": "model 'nope' not found" } });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E1002", .. }));
    }

    #[test]
    fn parse_response_falls_back_to_e1000_for_unrecognized_errors() {
        let body = serde_json::json!({ "error": { "message": "all backends exhausted" } });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E1000", .. }));
    }

    #[test]
    fn parse_response_rejects_missing_content() {
        assert!(parse_response(&serde_json::json!({})).is_err());
    }
}

/// Live smoke test against a real local OmniRoute process. Not run by
/// default — needs OmniRoute actually running on localhost:20128. Run
/// manually with:
///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored omniroute::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn send_reaches_a_real_local_omniroute_gateway() {
        let messages =
            vec![ChatMessage { role: "user".to_string(), content: "Reply with exactly one word: pong".to_string() }];
        // "auto" is OmniRoute's zero-config, keyless routing target — the
        // README's own quickstart example — so this needs nothing more
        // than the process running.
        let reply = send("auto", &messages).expect("live call to OmniRoute failed");
        assert!(!reply.trim().is_empty());
    }
}
