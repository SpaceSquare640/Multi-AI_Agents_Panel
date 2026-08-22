//! Colibrì adapter — a local, dependency-free MoE inference engine
//! (<https://github.com/JustVugg/colibri>) run via `coli serve`, which
//! exposes an OpenAI-compatible `/v1/chat/completions` endpoint on
//! `localhost:8000` by default. Same request/response shape as
//! `openai`/`openrouter`, but treated as a *local* provider like
//! `ollama` (no Key Vault entry, no cloud fallback rotation) — the user
//! runs their own `coli serve` process, there's no account or API key to
//! manage in the app.
//!
//! `coli serve` only enforces an API key if the user started it with
//! `COLI_API_KEY` set; this adapter sends one only if the *app's own*
//! process has `COLIBRI_API_KEY` set in its environment, matching how a
//! user would opt in to that on their own machine — never stored in Key
//! Vault, since it's a local secret shared between the app and a
//! process on the same machine, not a cloud credential.

use serde_json::Value;

use super::{ChatMessage, ProviderError};

const API_URL: &str = "http://localhost:8000/v1/chat/completions";

/// Builds the JSON request body — identical shape to `openai::build_request`
/// (colibrì's gateway is OpenAI-compatible), kept as its own pure function
/// so this adapter can evolve independently (e.g. colibrì-specific
/// extensions like `enable_thinking`) without touching `openai.rs`.
pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    super::openai::build_request(model, messages)
}

/// Extracts the assistant's reply, or classifies an error. Colibrì is a
/// local process, so errors are coded like Ollama's (E1xxx), not the
/// cloud E2xxx range — "model not found" (wrong `--model-id`) maps to
/// E1002, same meaning as Ollama's missing-local-model case.
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
    if let Ok(api_key) = std::env::var("COLIBRI_API_KEY") {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .json(&build_request(model, messages))
        .send()
        .map_err(|e| ProviderError::Network {
            error_code: "E1001",
            message: format!("could not reach colibrì (is `coli serve` running on localhost:8000?): {e}"),
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
        let body = build_request("glm-5.2-colibri", &messages);
        assert_eq!(body["model"], "glm-5.2-colibri");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn parse_response_extracts_text() {
        let body = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "ciao!" } }]
        });
        assert_eq!(parse_response(&body).unwrap(), "ciao!");
    }

    #[test]
    fn parse_response_classifies_model_not_found_as_e1002() {
        let body = serde_json::json!({ "error": { "message": "model 'nope' not found" } });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E1002", .. }));
    }

    #[test]
    fn parse_response_falls_back_to_e1000_for_unrecognized_errors() {
        let body = serde_json::json!({ "error": { "message": "queue saturated" } });
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E1000", .. }));
    }

    #[test]
    fn parse_response_rejects_missing_content() {
        assert!(parse_response(&serde_json::json!({})).is_err());
    }
}

/// Live smoke test against a real local `coli serve` process. Not run by
/// default — needs colibrì actually running on localhost:8000. Run
/// manually with:
///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored colibri::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn send_reaches_a_real_local_colibri_server() {
        let messages =
            vec![ChatMessage { role: "user".to_string(), content: "Reply with exactly one word: pong".to_string() }];
        // Model id must match whatever `--model-id` the running `coli
        // serve` process was started with.
        let reply = send("glm-5.2-colibri", &messages).expect("live call to colibri failed");
        assert!(!reply.trim().is_empty());
    }
}
