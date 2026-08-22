//! Optional second-pass classifier building block for the absolute-
//! prohibition screen (`screen_outgoing_message`'s keyword list), using
//! Meta's Llama Guard 3 run locally via Ollama — see
//! `Multi-AI Agent Panel Document/05 Research/Guardrails & Sandboxing
//! Upgrade Options.md` for why this was chosen over cloud moderation
//! APIs (stays offline-first, reuses the project's existing Ollama
//! integration, no new API key/account dependency).
//!
//! Wired into `send_message` via `guardrails::screen_with_llama_guard`,
//! gated behind the `LLAMA_GUARD_MODEL` environment variable — unset by
//! default, so an unconfigured install behaves exactly as before this
//! module existed (the keyword screen in `screen_outgoing_message`
//! remains the only mandatory, always-on check). **The HTTP round trip
//! itself is still only verified via the `live` test below, which needs
//! a real `llama-guard3` model pulled to actually run** — the parser
//! (`parse_llama_guard_response`) is fully unit-tested, but nobody has
//! run the `live` test in this repo's history yet. Anyone enabling
//! `LLAMA_GUARD_MODEL` is the first real verification this integration
//! gets against an actual model; `screen_with_llama_guard`'s fail-open
//! design means a broken integration degrades to "no extra check," not
//! "blocks everything," if that first real run surfaces a bug.

use serde_json::{json, Value};

const BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlamaGuardVerdict {
    Safe,
    /// Category codes as Llama Guard 3 reports them, e.g. `["S1", "S6"]`
    /// — see the MLCommons hazard taxonomy the model was trained on.
    Unsafe(Vec<String>),
    /// The response didn't match either expected shape — treated
    /// distinctly from `Safe` so a caller can decide whether to fail
    /// open or closed, rather than this module silently picking one.
    Unrecognized(String),
}

/// Parses Llama Guard 3's response text, which is always either the
/// literal line `safe` or `unsafe` followed by a newline and a
/// comma-separated category list (e.g. `unsafe\nS1,S6`). Pure function,
/// unit-testable without a real model.
pub fn parse_llama_guard_response(raw: &str) -> LlamaGuardVerdict {
    let trimmed = raw.trim();
    let mut lines = trimmed.lines();
    match lines.next().map(str::trim) {
        Some("safe") => LlamaGuardVerdict::Safe,
        Some("unsafe") => {
            let categories = lines
                .next()
                .map(|line| {
                    line.split(',')
                        .map(|c| c.trim().to_string())
                        .filter(|c| !c.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            LlamaGuardVerdict::Unsafe(categories)
        }
        _ => LlamaGuardVerdict::Unrecognized(trimmed.to_string()),
    }
}

/// Builds the `/api/chat` request body for a Llama Guard classification
/// call. Ollama's `llama-guard3` model applies its own built-in prompt
/// template (embedded in the model's Modelfile) to whatever's sent as a
/// plain user message — unlike a normal chat model, no extra
/// instruction wrapping is needed or expected. Pure function,
/// unit-testable.
pub fn build_classify_request(model: &str, text: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": [{ "role": "user", "content": text }],
    })
}

/// Calls a local `llama-guard3` model (via Ollama) to classify `text`.
/// **Not covered by a live test** — see module docs. Callers must treat
/// a `Err` (Ollama unreachable, model not pulled, malformed response) as
/// "classifier unavailable," not as a safety verdict either way.
pub fn classify(model: &str, text: &str) -> Result<LlamaGuardVerdict, String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{BASE_URL}/api/chat"))
        .json(&build_classify_request(model, text))
        .send()
        .map_err(|e| format!("could not reach Ollama: {e}"))?;

    let body: Value = response.json().map_err(|e| format!("could not parse Ollama response: {e}"))?;
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        return Err(err.to_string());
    }
    let content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| "response had no message content".to_string())?;
    Ok(parse_llama_guard_response(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_safe_verdict() {
        assert_eq!(parse_llama_guard_response("safe"), LlamaGuardVerdict::Safe);
    }

    #[test]
    fn parses_a_safe_verdict_with_surrounding_whitespace() {
        assert_eq!(parse_llama_guard_response("  safe\n"), LlamaGuardVerdict::Safe);
    }

    #[test]
    fn parses_an_unsafe_verdict_with_one_category() {
        assert_eq!(
            parse_llama_guard_response("unsafe\nS1"),
            LlamaGuardVerdict::Unsafe(vec!["S1".to_string()])
        );
    }

    #[test]
    fn parses_an_unsafe_verdict_with_multiple_categories() {
        assert_eq!(
            parse_llama_guard_response("unsafe\nS1,S6"),
            LlamaGuardVerdict::Unsafe(vec!["S1".to_string(), "S6".to_string()])
        );
    }

    #[test]
    fn parses_an_unsafe_verdict_with_spaced_categories() {
        assert_eq!(
            parse_llama_guard_response("unsafe\nS1, S6, S9"),
            LlamaGuardVerdict::Unsafe(vec!["S1".to_string(), "S6".to_string(), "S9".to_string()])
        );
    }

    #[test]
    fn treats_an_unsafe_verdict_with_no_category_line_as_an_empty_list_not_a_crash() {
        assert_eq!(parse_llama_guard_response("unsafe"), LlamaGuardVerdict::Unsafe(vec![]));
    }

    #[test]
    fn treats_unrecognized_text_as_unrecognized_not_safe() {
        // Fail-closed by construction: garbage output must not silently
        // resolve to `Safe` just because it didn't match "unsafe".
        match parse_llama_guard_response("I cannot classify this") {
            LlamaGuardVerdict::Unrecognized(_) => {}
            other => panic!("expected Unrecognized, got {other:?}"),
        }
    }

    #[test]
    fn treats_empty_response_as_unrecognized() {
        match parse_llama_guard_response("") {
            LlamaGuardVerdict::Unrecognized(_) => {}
            other => panic!("expected Unrecognized, got {other:?}"),
        }
    }

    #[test]
    fn build_classify_request_shapes_a_single_user_message() {
        let body = build_classify_request("llama-guard3:1b", "some text to classify");
        assert_eq!(body["model"], "llama-guard3:1b");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "some text to classify");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }
}

/// Live smoke test against a real local `llama-guard3` model. Not run by
/// default. Run manually with:
///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored llama_guard::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn classifies_an_obviously_safe_message() {
        let verdict = classify("llama-guard3:1b", "What's a good recipe for banana bread?").unwrap();
        assert_eq!(verdict, LlamaGuardVerdict::Safe);
    }
}
