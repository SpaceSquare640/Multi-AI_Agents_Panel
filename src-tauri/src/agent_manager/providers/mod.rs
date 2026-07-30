//! Cloud/local provider adapters. Each adapter translates the app's
//! internal `ChatMessage` shape to/from a specific provider's API, so the
//! rest of the app never needs to know which provider it's talking to.
//! Design: `Multi-AI Agent Panel Document/03 Development Notes/Architecture.md`

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod openrouter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// "user" | "assistant"
    pub role: String,
    pub content: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProviderError {
    /// The provider was reachable but returned an error response.
    Api(String),
    /// Couldn't reach the provider at all (DNS, timeout, TLS, ...).
    Network(String),
    /// No adapter exists yet for this `provider_name`.
    Unsupported(String),
    /// Blocked by `guardrails` before any provider was ever contacted.
    /// Carries the Error Code Registry code (e.g. "E9002").
    GuardrailBlocked { error_code: &'static str, reason: String },
    /// Every candidate `fallback::run_with_fallback` tried failed (or
    /// there were none to try). Carries the Error Code Registry code
    /// ("E3001") and a description of each attempt.
    AllProvidersFailed { error_code: &'static str, attempts: Vec<String> },
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Api(msg) => write!(f, "provider returned an error: {msg}"),
            ProviderError::Network(msg) => write!(f, "could not reach provider: {msg}"),
            ProviderError::Unsupported(name) => write!(f, "no provider adapter for '{name}' yet"),
            ProviderError::GuardrailBlocked { error_code, reason } => {
                write!(f, "{error_code} blocked by Guardrails: {reason}")
            }
            ProviderError::AllProvidersFailed { error_code, attempts } => {
                write!(f, "{error_code} all providers failed: {}", attempts.join("; "))
            }
        }
    }
}
