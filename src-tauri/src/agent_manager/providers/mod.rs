//! Cloud/local provider adapters. Each adapter translates the app's
//! internal `ChatMessage` shape to/from a specific provider's API, so the
//! rest of the app never needs to know which provider it's talking to.
//! Design: `Multi-AI Agent Panel Document/03 Development Notes/Architecture.md`

pub mod anthropic;
pub mod colibri;
pub mod ollama;
pub mod omniroute;
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
    /// `error_code` is an Error Code Registry E1xxx (local) or E2xxx
    /// (cloud) code — see `classify_cloud_api_error` for how cloud
    /// adapters pick one from the provider's own error response.
    Api { error_code: &'static str, message: String },
    /// Couldn't reach the provider at all (DNS, timeout, TLS, ...).
    Network { error_code: &'static str, message: String },
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
            ProviderError::Api { error_code, message } => {
                write!(f, "{error_code} provider returned an error: {message}")
            }
            ProviderError::Network { error_code, message } => {
                write!(f, "{error_code} could not reach provider: {message}")
            }
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

/// Maps a cloud provider's own error signal to an Error Code Registry
/// E2xxx code (see `Error Code Registry.md`'s E2xxx table). `numeric_code`
/// is an HTTP-style status if the provider's error body includes one
/// (OpenRouter does); `signal` is whatever text fields are available
/// (Anthropic's `error.type`, OpenAI's `error.type`/`error.code`, or a
/// provider's plain error message) concatenated together — checked as a
/// fallback/supplement, not instead of, the numeric code. Unrecognized
/// signals map to E2000 (the registry's documented catch-all for this
/// range) rather than guessing.
pub fn classify_cloud_api_error(signal: &str, numeric_code: Option<i64>) -> &'static str {
    if let Some(code) = numeric_code {
        match code {
            401 | 403 => return "E2001",
            429 => return "E2002",
            404 => return "E2005",
            500..=599 => return "E2004",
            _ => {}
        }
    }
    let s = signal.to_lowercase();
    if s.contains("auth") || s.contains("api key") || s.contains("api_key") || s.contains("permission") {
        "E2001"
    } else if s.contains("rate_limit") || s.contains("rate limit") || s.contains("quota") {
        "E2002"
    } else if s.contains("overloaded") || s.contains("server_error") || s.contains("internal") {
        "E2004"
    } else if s.contains("not_found") || s.contains("not found") {
        "E2005"
    } else {
        "E2000"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cloud_api_error_prefers_a_numeric_http_status_when_available() {
        assert_eq!(classify_cloud_api_error("some vague message", Some(401)), "E2001");
        assert_eq!(classify_cloud_api_error("some vague message", Some(429)), "E2002");
        assert_eq!(classify_cloud_api_error("some vague message", Some(404)), "E2005");
        assert_eq!(classify_cloud_api_error("some vague message", Some(503)), "E2004");
    }

    #[test]
    fn classify_cloud_api_error_falls_back_to_text_signals() {
        assert_eq!(classify_cloud_api_error("authentication_error", None), "E2001");
        assert_eq!(classify_cloud_api_error("invalid api key", None), "E2001");
        assert_eq!(classify_cloud_api_error("rate_limit_error", None), "E2002");
        assert_eq!(classify_cloud_api_error("model_not_found", None), "E2005");
        assert_eq!(classify_cloud_api_error("overloaded_error", None), "E2004");
    }

    #[test]
    fn classify_cloud_api_error_defaults_to_the_e2000_catch_all() {
        assert_eq!(classify_cloud_api_error("something completely unrecognized", None), "E2000");
    }
}
