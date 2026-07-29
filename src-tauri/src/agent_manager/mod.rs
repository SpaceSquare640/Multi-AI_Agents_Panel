//! Agent Manager: creates/switches/destroys agent instances and unifies
//! local and cloud providers behind one interface.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Agent Registry.md`

pub mod providers;

use crate::key_vault;
use crate::storage::Agent;
pub use providers::{ChatMessage, ProviderError};

/// Looks up a cloud provider's API key in the Key Vault, turning "not set"
/// and "vault error" into the same `ProviderError::Api` shape callers
/// already handle.
fn cloud_key(provider_name: &str) -> Result<String, ProviderError> {
    key_vault::get_api_key(provider_name)
        .map_err(|e| ProviderError::Api(format!("key vault error: {e}")))?
        .ok_or_else(|| ProviderError::Api(format!("no {provider_name} API key set in the Key Vault")))
}

/// Sends `messages` to whichever provider `agent` is configured to use,
/// fetching its API key from the Key Vault along the way. This is the one
/// place in the app that knows how to go from a stored `Agent` to an
/// actual provider call — everything above this layer just deals in
/// `Agent`s and `ChatMessage`s.
pub fn send_message(agent: &Agent, messages: &[ChatMessage]) -> Result<String, ProviderError> {
    match agent.provider_name.as_str() {
        "anthropic" => {
            let api_key = cloud_key("anthropic")?;
            providers::anthropic::send(&api_key, &agent.model, messages)
        }
        "openrouter" => {
            let api_key = cloud_key("openrouter")?;
            providers::openrouter::send(&api_key, &agent.model, messages)
        }
        other => Err(ProviderError::Unsupported(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_with_provider(provider_name: &str) -> Agent {
        Agent {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            role_template: None,
            provider_kind: "cloud".to_string(),
            provider_name: provider_name.to_string(),
            model: "claude-sonnet".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn unsupported_provider_is_rejected_before_any_network_call() {
        let agent = agent_with_provider("some-provider-with-no-adapter-yet");
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let err = send_message(&agent, &messages).unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(name) if name == "some-provider-with-no-adapter-yet"));
    }
}
