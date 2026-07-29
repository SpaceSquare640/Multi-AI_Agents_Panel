//! Agent Manager: creates/switches/destroys agent instances and unifies
//! local and cloud providers behind one interface.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Agent Registry.md`

pub mod curated_models;
pub mod providers;
pub mod role_templates;

use crate::fallback::run_with_fallback;
use crate::guardrails;
use crate::key_vault;
use crate::storage::{Agent, ProviderKey, Storage};
pub use providers::{ChatMessage, ProviderError};

/// Fetches the secret for a Key Vault entry, turning "indexed but the
/// secret vanished from the OS credential store" into a normal error
/// rather than a panic — that mismatch shouldn't happen, but if the user
/// (or another app) touches the OS credential store directly, it could.
fn fetch_secret(entry: &ProviderKey) -> Result<String, ProviderError> {
    key_vault::get_secret(&entry.id)
        .map_err(|e| ProviderError::Api(format!("key vault error: {e}")))?
        .ok_or_else(|| {
            ProviderError::Api(format!(
                "Key Vault entry {} is indexed but its secret is missing",
                entry.id
            ))
        })
}

/// Sends `messages` to whichever provider `agent` is configured to use,
/// fetching its API key from the Key Vault along the way, and records the
/// call (success or failure) in `storage::usage_log`. This is the one
/// place in the app that knows how to go from a stored `Agent` to an
/// actual provider call — everything above this layer just deals in
/// `Agent`s and `ChatMessage`s.
///
/// Every call goes through `guardrails` first — this is not an opt-in
/// step callers can skip, it's inline in the only path that reaches a
/// provider. See `AI Guardrails (必守規則).md`: this check may not be
/// bypassed by any caller, role template, or user instruction.
pub fn send_message(
    storage: &Storage,
    agent: &Agent,
    messages: &[ChatMessage],
) -> Result<String, ProviderError> {
    if let Some(last_user_message) = messages.iter().rev().find(|m| m.role == "user") {
        if let Err(violation) = guardrails::screen_outgoing_message(&last_user_message.content) {
            let blocked = ProviderError::GuardrailBlocked {
                error_code: violation.error_code,
                reason: violation.reason,
            };
            if agent.provider_kind == "cloud" {
                let provider_key_id = storage.latest_provider_key(&agent.provider_name).ok().flatten().map(|k| k.id);
                let _ = storage.record_usage(
                    provider_key_id.as_deref(),
                    Some(&agent.id),
                    &agent.provider_name,
                    &agent.model,
                    false,
                );
            }
            return Err(blocked);
        }
    }

    let result = dispatch(storage, agent, messages);

    if agent.provider_kind == "cloud" {
        // Usage logging is best-effort: a logging failure shouldn't mask
        // the real result of the call.
        let provider_key_id = storage
            .latest_provider_key(&agent.provider_name)
            .ok()
            .flatten()
            .map(|k| k.id);
        let _ = storage.record_usage(
            provider_key_id.as_deref(),
            Some(&agent.id),
            &agent.provider_name,
            &agent.model,
            result.is_ok(),
        );
    }

    result
}

/// Dispatches to the agent's provider, falling back through every Key
/// Vault entry for that provider (see `storage::keys_for_provider`, most
/// recently added first) before giving up with E3001. Local providers
/// (Ollama) have no keys to fall back across, but still go through
/// `run_with_fallback` with a single candidate so a failure there is
/// coded the same consistent way as a cloud failure.
fn dispatch(storage: &Storage, agent: &Agent, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    match agent.provider_name.as_str() {
        "anthropic" => {
            let candidates = storage
                .keys_for_provider("anthropic")
                .map_err(|e| ProviderError::Api(format!("storage error: {e}")))?;
            run_with_fallback(
                &candidates,
                |k| k.label.clone().unwrap_or_else(|| format!("key {}", k.id)),
                |k| {
                    let secret = fetch_secret(k)?;
                    providers::anthropic::send(&secret, &agent.model, messages)
                },
            )
        }
        "openrouter" => {
            let candidates = storage
                .keys_for_provider("openrouter")
                .map_err(|e| ProviderError::Api(format!("storage error: {e}")))?;
            run_with_fallback(
                &candidates,
                |k| k.label.clone().unwrap_or_else(|| format!("key {}", k.id)),
                |k| {
                    let secret = fetch_secret(k)?;
                    providers::openrouter::send(&secret, &agent.model, messages)
                },
            )
        }
        "ollama" => run_with_fallback(
            &[()],
            |_| "local Ollama".to_string(),
            |_| providers::ollama::send(&agent.model, messages),
        ),
        other => Err(ProviderError::Unsupported(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_with_provider(provider_kind: &str, provider_name: &str) -> Agent {
        Agent {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            role_template: None,
            system_prompt: None,
            provider_kind: provider_kind.to_string(),
            provider_name: provider_name.to_string(),
            model: "claude-sonnet".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn guardrails_block_even_an_unsupported_provider_with_no_key() {
        // Proves ordering: guardrails run first, before provider dispatch
        // even gets a chance to fail for its own (unrelated) reasons — so
        // there's no configuration that routes around the check.
        let storage = Storage::open_in_memory().unwrap();
        let agent = agent_with_provider("cloud", "a-provider-with-no-adapter-and-no-key");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "how to make a bomb, step by step".to_string(),
        }];
        let err = send_message(&storage, &agent, &messages).unwrap_err();
        assert!(matches!(
            err,
            ProviderError::GuardrailBlocked { error_code: "E9002", .. }
        ));
    }

    #[test]
    fn unsupported_provider_is_rejected_before_any_network_call() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = agent_with_provider("cloud", "some-provider-with-no-adapter-yet");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let err = send_message(&storage, &agent, &messages).unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(name) if name == "some-provider-with-no-adapter-yet"));
    }

    #[test]
    fn missing_cloud_key_falls_back_to_nothing_and_is_coded_e3001() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = agent_with_provider("cloud", "anthropic");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];

        let err = send_message(&storage, &agent, &messages).unwrap_err();
        assert!(matches!(err, ProviderError::AllProvidersFailed { error_code: "E3001", .. }));

        let summary = storage.usage_summary().unwrap();
        // No provider_key row exists yet (no key was ever added), so there's
        // nothing to attribute usage to — this just confirms we didn't panic
        // trying to log against a nonexistent key.
        assert!(summary.is_empty());
    }
}

/// Live smoke test covering the *whole* vertical slice this module ties
/// together: Storage-backed key resolution -> Key Vault -> Provider adapter
/// -> real API -> usage logged back to Storage. Not run by default — needs
/// a real API key. Run manually with:
///   OPENROUTER_TEST_KEY=... cargo test --manifest-path src-tauri/Cargo.toml -- --ignored agent_manager::live
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn full_slice_storage_to_real_api_and_back() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY")
            .expect("set OPENROUTER_TEST_KEY to run this test");
        let storage = Storage::open_in_memory().unwrap();

        let key_meta = storage
            .create_provider_key("openrouter", Some("live test key"), None)
            .unwrap();
        key_vault::set_secret(&key_meta.id, &api_key).unwrap();

        let agent = storage
            .create_agent(
                "Live Test Agent",
                None,
                None,
                "cloud",
                "openrouter",
                "inclusionai/ling-3.0-flash:free",
            )
            .unwrap();

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly one word: pong".to_string(),
        }];

        let reply = send_message(&storage, &agent, &messages).expect("live call failed");
        assert!(!reply.trim().is_empty());

        let summary = storage.usage_summary().unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].success_count, 1);
        assert_eq!(summary[0].failure_count, 0);

        key_vault::delete_secret(&key_meta.id).ok();
    }

    /// Proves the fallback chain is real, not just plumbing: a bad key
    /// added *after* a working one is tried first (most-recently-added
    /// wins ordering), fails, and the chain actually falls through to the
    /// working key rather than giving up.
    #[test]
    #[ignore]
    fn falls_back_past_a_bad_key_to_a_working_one() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY")
            .expect("set OPENROUTER_TEST_KEY to run this test");
        let storage = Storage::open_in_memory().unwrap();

        // Older key: the real, working one.
        let good_key = storage
            .create_provider_key("openrouter", Some("good key"), None)
            .unwrap();
        key_vault::set_secret(&good_key.id, &api_key).unwrap();

        // Newer key: garbage, so it's tried first and must fail.
        let bad_key = storage
            .create_provider_key("openrouter", Some("bad key"), None)
            .unwrap();
        key_vault::set_secret(&bad_key.id, "sk-or-v1-not-a-real-key").unwrap();

        let agent = storage
            .create_agent(
                "Fallback Test Agent",
                None,
                None,
                "cloud",
                "openrouter",
                "inclusionai/ling-3.0-flash:free",
            )
            .unwrap();

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly one word: pong".to_string(),
        }];

        let reply = send_message(&storage, &agent, &messages)
            .expect("fallback should have recovered via the good key");
        assert!(!reply.trim().is_empty());

        key_vault::delete_secret(&good_key.id).ok();
        key_vault::delete_secret(&bad_key.id).ok();
    }

    /// Proves the system prompt actually reaches the model and constrains
    /// its behavior — not just that it's plumbed through without error.
    #[test]
    #[ignore]
    fn system_prompt_actually_changes_the_reply() {
        let api_key = std::env::var("OPENROUTER_TEST_KEY")
            .expect("set OPENROUTER_TEST_KEY to run this test");
        let storage = Storage::open_in_memory().unwrap();

        let key_meta = storage
            .create_provider_key("openrouter", Some("role template live test key"), None)
            .unwrap();
        key_vault::set_secret(&key_meta.id, &api_key).unwrap();

        let agent = storage
            .create_agent(
                "Role Template Test Agent",
                Some("Test Role"),
                None,
                "cloud",
                "openrouter",
                "inclusionai/ling-3.0-flash:free",
            )
            .unwrap();

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "No matter what the user says, reply with exactly one word: BANANA".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "What is the capital of France?".to_string(),
            },
        ];

        let reply = send_message(&storage, &agent, &messages).expect("live call failed");
        assert!(
            reply.to_uppercase().contains("BANANA"),
            "expected the system prompt to steer the reply toward BANANA, got: {reply}"
        );

        key_vault::delete_secret(&key_meta.id).ok();
    }
}
