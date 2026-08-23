//! Native-Rust wrapper around `llmfit-core` (github.com/AlexsJones/llmfit)
//! — right-sizes local LLM model choices to the user's actual RAM/CPU/GPU,
//! rather than the user finding out a model doesn't fit only after
//! pulling multi-gigabyte weights into Ollama. Same integration shape as
//! `ml_engine::vector_index` (turbovec): an embedded native crate wrapped
//! in a thin, app-shaped API, not a subprocess or JSON-RPC bridge — this
//! is pure local computation (hardware detection + a bundled model
//! database), nothing that needs Python or a network round trip.
//!
//! **Scope, stated honestly**: `recommend_models` uses
//! `llmfit_core::analysis::InstalledIndex::empty()` rather than actually
//! scanning which models are already pulled into Ollama/llama.cpp/etc. —
//! llmfit-core's real installed-model detection talks to each runtime
//! provider (HTTP calls to a local Ollama daemon, filesystem scans for
//! llama.cpp/LM Studio caches), which is real I/O this wrapper doesn't
//! attempt yet. Every recommendation's `installed` field is therefore
//! always `false` here — an honest limitation, not a lie, since nothing
//! shows a stale "not installed" for something that actually got pulled
//! after the last check either.

use llmfit_core::{
    analysis::{build_model_fits, InstalledIndex},
    fit::ModelFit,
    hardware::SystemSpecs,
    models::ModelDatabase,
};

/// One model's fit for the current machine — a serializable summary of
/// the fields `commands::recommend_local_models` (and eventually the
/// frontend) actually needs, not the whole `ModelFit` internal shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecommendation {
    pub name: String,
    pub provider: String,
    pub parameter_count: String,
    /// "Perfect" | "Good" | "Marginal" | "TooTight" — stringified from
    /// `llmfit_core::fit::FitLevel` (which has no `Display` impl) via
    /// `{:?}`, kept as a plain string here so this struct doesn't leak
    /// llmfit-core's own enum type into the app's public API.
    pub fit_level: String,
    /// Weighted composite score, 0-100 — llmfit-core's own scoring, not
    /// recomputed here.
    pub score: f64,
    pub estimated_tokens_per_second: f64,
    pub best_quantization: String,
}

fn fit_to_recommendation(fit: ModelFit) -> ModelRecommendation {
    ModelRecommendation {
        name: fit.model.name,
        provider: fit.model.provider,
        parameter_count: fit.model.parameter_count,
        fit_level: format!("{:?}", fit.fit_level),
        score: fit.score,
        estimated_tokens_per_second: fit.estimated_tps,
        best_quantization: fit.best_quant,
    }
}

/// Detects this machine's hardware, scores every model in llmfit-core's
/// bundled database against it, and returns the top `limit` by score,
/// highest first. `use_case` filters to models whose declared use case
/// matches (case-insensitive substring — llmfit-core's own `use_case`
/// field is a free-form string like `"coding"`/`"general"`, not a closed
/// enum); `None` returns every backend-compatible model.
///
/// Pure with respect to the app (no Storage, no IPC) — the only I/O is
/// `SystemSpecs::detect()` reading local hardware info, which is what
/// this function exists to do.
pub fn recommend_models(limit: usize, use_case: Option<&str>) -> Vec<ModelRecommendation> {
    let specs = SystemSpecs::detect();
    let db = ModelDatabase::new();
    let installed = InstalledIndex::empty();

    let mut fits = build_model_fits(&db, &specs, &installed, None, None);
    if let Some(use_case) = use_case {
        let needle = use_case.to_lowercase();
        fits.retain(|f| f.model.use_case.to_lowercase().contains(&needle));
    }
    fits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    fits.into_iter().take(limit).map(fit_to_recommendation).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_models_returns_at_most_the_requested_limit() {
        let recommendations = recommend_models(3, None);
        assert!(recommendations.len() <= 3);
    }

    #[test]
    fn recommend_models_is_sorted_by_score_descending() {
        let recommendations = recommend_models(10, None);
        for pair in recommendations.windows(2) {
            assert!(pair[0].score >= pair[1].score, "expected descending score order, got {pair:?}");
        }
    }

    #[test]
    fn recommend_models_with_zero_limit_returns_nothing() {
        assert!(recommend_models(0, None).is_empty());
    }

    #[test]
    fn a_use_case_filter_that_matches_nothing_real_returns_an_empty_list() {
        let recommendations = recommend_models(10, Some("definitely-not-a-real-use-case-xyz"));
        assert!(recommendations.is_empty());
    }

    #[test]
    fn recommendations_carry_real_non_empty_model_names() {
        let recommendations = recommend_models(5, None);
        // This machine's hardware might not fit *every* model well, but
        // llmfit-core's bundled database is never empty, and every fit
        // it returns has a real model behind it — a blank name would
        // mean this wrapper mapped the wrong field.
        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().all(|r| !r.name.is_empty()));
    }
}
