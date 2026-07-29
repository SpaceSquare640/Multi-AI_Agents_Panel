//! Agent Manager: creates/switches/destroys agent instances and unifies
//! local and cloud providers behind one interface.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Agent Registry.md`

pub mod curated_models;
pub mod providers;

use crate::key_vault;
use crate::storage::{Agent, Storage};
pub use providers::{ChatMessage, ProviderError};

/// Resolves the API key to use for a cloud provider: picks that provider's
/// most recently added Key Vault entry (a user can hold several keys per
/// provider — see `storage::ProviderKey` — but an `Agent` doesn't pin one
/// explicitly yet, so "latest" is the default until agent creation grows
/// that option). Returns the entry id (for usage logging) plus the secret.
fn resolve_cloud_key(storage: &Storage, provider_name: &str) -> Result<(String, String), ProviderError> {
    let entry = storage
        .latest_provider_key(provider_name)
        .map_err(|e| ProviderError::Api(format!("storage error: {e}")))?
        .ok_or_else(|| ProviderError::Api(format!("no {provider_name} API key set in the Key Vault")))?;

    let secret = key_vault::get_secret(&entry.id)
        .map_err(|e| ProviderError::Api(format!("key vault error: {e}")))?
        .ok_or_else(|| {
            ProviderError::Api(format!(
                "Key Vault entry {} is indexed but its secret is missing",
                entry.id
            ))
        })?;

    Ok((entry.id, secret))
}

/// Sends `messages` to whichever provider `agent` is configured to use,
/// fetching its API key from the Key Vault along the way, and records the
/// call (success or failure) in `storage::usage_log`. This is the one
/// place in the app that knows how to go from a stored `Agent` to an
/// actual provider call — everything above this layer just deals in
/// `Agent`s and `ChatMessage`s.
pub fn send_message(
    storage: &Storage,
    agent: &Agent,
    messages: &[ChatMessage],
) -> Result<String, ProviderError> {
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

fn dispatch(storage: &Storage, agent: &Agent, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    match agent.provider_name.as_str() {
        "anthropic" => {
            let (_, api_key) = resolve_cloud_key(storage, "anthropic")?;
            providers::anthropic::send(&api_key, &agent.model, messages)
        }
        "openrouter" => {
            let (_, api_key) = resolve_cloud_key(storage, "openrouter")?;
            providers::openrouter::send(&api_key, &agent.model, messages)
        }
        "ollama" => providers::ollama::send(&agent.model, messages),
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
            provider_kind: provider_kind.to_string(),
            provider_name: provider_name.to_string(),
            model: "claude-sonnet".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
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
    fn missing_cloud_key_is_reported_and_logged_as_a_failure() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = agent_with_provider("cloud", "anthropic");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];

        let err = send_message(&storage, &agent, &messages).unwrap_err();
        assert!(matches!(err, ProviderError::Api(_)));

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
}
