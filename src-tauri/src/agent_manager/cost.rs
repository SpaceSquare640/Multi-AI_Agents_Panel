//! Cost estimation: turns a provider-reported token count into an
//! estimated USD cost, given per-million-token pricing. Answers one of
//! the Usage dashboard's open items (see the vault's Daily Log
//! "待釐清" section: "用量儀表板的「估計花費」功能仍未實作").
//!
//! **Staged building block, not yet wired into the live dispatch
//! path** (`#![allow(dead_code)]` below, same convention as
//! `ml_engine::vector_index`): `storage::usage_log` now has nullable
//! `prompt_tokens`/`completion_tokens`/`estimated_cost_usd` columns and
//! `Storage::record_usage_with_cost`, and `providers::openrouter` can
//! parse token usage from a response (`send_with_usage`/`parse_usage`)
//! — OpenRouter is the one provider this codebase already has live
//! per-model pricing for (`openrouter_catalog`). What's still missing
//! is wiring `send_with_usage` into `agent_manager::dispatch_one`'s
//! `OpenRouter` branch without double-logging: `fallback::run_with_fallback`'s
//! `on_attempt` callback only carries a success bool today, not the `Ok`
//! value, so cost can't reach the same `usage_log` row a plain
//! `record_usage` call already writes for every attempt — widening
//! `on_attempt`'s signature to carry the outcome is a real, separate
//! change deserving its own focused pass, not squeezed in here as an
//! afterthought that risks silently duplicating usage rows.
//! Anthropic/OpenAI would additionally need their own usage-parsing +
//! a pricing source (Anthropic's response body already reports
//! `usage.input_tokens`/`output_tokens`, but there's no equivalent live
//! pricing catalog for it in this repo yet — a static table would drift
//! out of date silently, which this project's "don't fake correctness"
//! convention prefers not to ship). Local providers (Ollama/colibrì/
//! OmniRoute) have no cost at all — not part of this by design.

#![allow(dead_code)]

/// `prompt_price_per_million`/`completion_price_per_million` are USD per
/// 1,000,000 tokens (the unit `openrouter_catalog::CuratedModel` already
/// stores pricing in) — `None` for either means "this model's price for
/// that token type isn't known," and the whole estimate is `None`
/// rather than silently estimating half a call's cost as zero.
pub fn estimate_usd(
    usage: &crate::agent_manager::providers::TokenUsage,
    prompt_price_per_million: Option<f64>,
    completion_price_per_million: Option<f64>,
) -> Option<f64> {
    let prompt_price = prompt_price_per_million?;
    let completion_price = completion_price_per_million?;
    let prompt_cost = usage.prompt_tokens as f64 / 1_000_000.0 * prompt_price;
    let completion_cost = usage.completion_tokens as f64 / 1_000_000.0 * completion_price;
    Some(prompt_cost + completion_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_manager::providers::TokenUsage;

    #[test]
    fn estimates_cost_from_prompt_and_completion_pricing() {
        let usage = TokenUsage { prompt_tokens: 1_000_000, completion_tokens: 1_000_000 };
        let cost = estimate_usd(&usage, Some(3.0), Some(15.0)).unwrap();
        assert!((cost - 18.0).abs() < 1e-9);
    }

    #[test]
    fn scales_linearly_with_a_partial_million_tokens() {
        let usage = TokenUsage { prompt_tokens: 500_000, completion_tokens: 0 };
        let cost = estimate_usd(&usage, Some(3.0), Some(15.0)).unwrap();
        assert!((cost - 1.5).abs() < 1e-9);
    }

    #[test]
    fn is_none_when_prompt_price_is_unknown() {
        let usage = TokenUsage { prompt_tokens: 100, completion_tokens: 100 };
        assert_eq!(estimate_usd(&usage, None, Some(15.0)), None);
    }

    #[test]
    fn is_none_when_completion_price_is_unknown() {
        let usage = TokenUsage { prompt_tokens: 100, completion_tokens: 100 };
        assert_eq!(estimate_usd(&usage, Some(3.0), None), None);
    }

    #[test]
    fn zero_tokens_is_a_real_zero_cost_not_none() {
        let usage = TokenUsage { prompt_tokens: 0, completion_tokens: 0 };
        assert_eq!(estimate_usd(&usage, Some(3.0), Some(15.0)), Some(0.0));
    }
}
