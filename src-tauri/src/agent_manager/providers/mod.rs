//! Cloud/local provider adapters. Each adapter translates the app's
//! internal `ChatMessage` shape to/from a specific provider's API, so the
//! rest of the app never needs to know which provider it's talking to.
//! Design: `Multi-AI Agent Panel Document/03 Development Notes/Architecture.md`

pub mod anthropic;
pub mod ollama;
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
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Api(msg) => write!(f, "provider returned an error: {msg}"),
            ProviderError::Network(msg) => write!(f, "could not reach provider: {msg}"),
            ProviderError::Unsupported(name) => write!(f, "no provider adapter for '{name}' yet"),
        }
    }
}
