//! Cost estimation: turns a provider-reported token count into an
//! estimated USD cost, given per-million-token pricing.
//!
//! `agent_manager::dispatch_one`'s `OpenRouter` branch now records real
//! `prompt_tokens`/`completion_tokens` per call (via
//! `providers::openrouter::send_with_usage`) into `storage::usage_log` —
//! OpenRouter is the one provider this codebase already has live
//! per-model pricing for (`openrouter_catalog`). The actual USD estimate
//! is computed at *read* time (`commands::get_usage_summary_with_cost`,
//! via `Storage::usage_log_rows_for_key` + this module's `estimate_usd`),
//! not stored per-call — this joins each recorded call's tokens against
//! *current* pricing when the Usage dashboard is actually viewed, rather
//! than freezing a possibly-stale price at call time. `usage_log`'s own
//! `estimated_cost_usd` column exists (`record_usage_with_cost`'s
//! signature accepts it) but is currently always written as `None` for
//! exactly this reason — nothing pre-computes it at insert time.
//!
//! **Scope, stated honestly**: Anthropic/OpenAI would additionally need
//! their own usage-parsing + a pricing source (Anthropic's response body
//! already reports `usage.input_tokens`/`output_tokens`, but there's no
//! equivalent live pricing catalog for it in this repo yet — a static
//! table would drift out of date silently, which this project's "don't
//! fake correctness" convention prefers not to ship). Local providers
//! (Ollama/colibrì/OmniRoute) have no cost at all — not part of this by
//! design.

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

/// Sums the estimated cost of a key's recorded calls (`(model,
/// prompt_tokens, completion_tokens)`, e.g. from
/// `Storage::usage_log_rows_for_key`) against a pricing lookup keyed by
/// model id — the pure aggregation `commands::get_usage_summary_with_cost`
/// runs per OpenRouter key, split out here so it's testable without a
/// database or the OpenRouter catalog's cache/network. `None` when zero
/// rows had both known tokens and known pricing (nothing to estimate
/// from), never a false `Some(0.0)`.
pub fn estimate_total_usd(
    rows: &[(String, Option<u32>, Option<u32>)],
    pricing: &std::collections::HashMap<String, (Option<f64>, Option<f64>)>,
) -> Option<f64> {
    let mut total = 0.0;
    let mut any_known = false;
    for (model, prompt_tokens, completion_tokens) in rows {
        let (Some(prompt_tokens), Some(completion_tokens)) = (prompt_tokens, completion_tokens) else {
            continue;
        };
        let Some((prompt_price, completion_price)) = pricing.get(model) else {
            continue;
        };
        let usage = crate::agent_manager::providers::TokenUsage {
            prompt_tokens: *prompt_tokens,
            completion_tokens: *completion_tokens,
        };
        if let Some(cost) = estimate_usd(&usage, *prompt_price, *completion_price) {
            total += cost;
            any_known = true;
        }
    }
    any_known.then_some(total)
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

    fn pricing(entries: &[(&str, f64, f64)]) -> std::collections::HashMap<String, (Option<f64>, Option<f64>)> {
        entries.iter().map(|(id, p, c)| (id.to_string(), (Some(*p), Some(*c)))).collect()
    }

    #[test]
    fn estimate_total_usd_sums_across_multiple_rows_of_the_same_model() {
        let rows = vec![
            ("model-a".to_string(), Some(1_000_000), Some(0)),
            ("model-a".to_string(), Some(1_000_000), Some(0)),
        ];
        let prices = pricing(&[("model-a", 3.0, 15.0)]);
        assert_eq!(estimate_total_usd(&rows, &prices), Some(6.0));
    }

    #[test]
    fn estimate_total_usd_skips_rows_with_unknown_tokens_but_still_counts_the_rest() {
        let rows = vec![
            ("model-a".to_string(), None, None),
            ("model-a".to_string(), Some(1_000_000), Some(0)),
        ];
        let prices = pricing(&[("model-a", 3.0, 0.0)]);
        assert_eq!(estimate_total_usd(&rows, &prices), Some(3.0));
    }

    #[test]
    fn estimate_total_usd_skips_rows_whose_model_has_no_known_pricing() {
        let rows = vec![("unpriced-model".to_string(), Some(1_000_000), Some(0))];
        let prices = pricing(&[("some-other-model", 3.0, 15.0)]);
        assert_eq!(estimate_total_usd(&rows, &prices), None);
    }

    #[test]
    fn estimate_total_usd_is_none_for_an_empty_row_list() {
        let prices = pricing(&[]);
        assert_eq!(estimate_total_usd(&[], &prices), None);
    }
}
