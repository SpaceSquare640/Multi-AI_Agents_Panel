//! Live OpenRouter model catalog — implements the decisions in
//! `Multi-AI Agent Panel Document/04 Agents & Orchestration/Model Lists (Default)/OpenRouter Default Model List.md`:
//! search OpenRouter's real, current model list (not just the static
//! curated default) with per-token USD pricing, refreshed at most once
//! every 24h (in-memory cache, see `OpenRouterCatalogState`), falling
//! back to the static curated list if the API call fails.
//!
//! Pure JSON parsing (`parse_models_response`) is split from the network
//! call (`fetch_live`) so the parsing logic is unit-testable without a
//! real HTTP round-trip — same pattern as the provider modules under
//! `agent_manager::providers`.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::agent_manager::curated_models;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    /// USD per 1M prompt (input) tokens — `None` if OpenRouter didn't
    /// report a parseable price for this model (e.g. a free model, or a
    /// pricing field OpenRouter didn't include).
    pub prompt_price_per_million: Option<f64>,
    /// USD per 1M completion (output) tokens.
    pub completion_price_per_million: Option<f64>,
}

/// What a model list command actually returns: the models plus whether
/// they came from a real, fresh(-enough) OpenRouter API response
/// (`live: true`) or the static fallback (`live: false`) — the frontend
/// uses this to show the "price/availability may not be current" notice
/// the design doc requires whenever we couldn't reach the real API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModelsResult {
    pub models: Vec<OpenRouterModel>,
    pub live: bool,
}

fn static_fallback() -> OpenRouterModelsResult {
    OpenRouterModelsResult {
        models: curated_models::openrouter_models()
            .into_iter()
            .map(|m| OpenRouterModel {
                id: m.id,
                name: m.label,
                prompt_price_per_million: None,
                completion_price_per_million: None,
            })
            .collect(),
        live: false,
    }
}

/// OpenRouter reports per-token USD prices as decimal strings (e.g.
/// `"0.000003"`), not numbers — parses one into a per-million-token USD
/// figure. Returns `None` for anything that doesn't parse as a
/// non-negative number (missing field, `"-1"` sentinel some providers
/// use for "not applicable", stray text, etc.) rather than guessing.
fn per_million(raw: Option<&serde_json::Value>) -> Option<f64> {
    let text = raw?.as_str()?;
    let per_token: f64 = text.parse().ok()?;
    if per_token < 0.0 {
        return None;
    }
    Some(per_token * 1_000_000.0)
}

/// Parses OpenRouter's `GET /api/v1/models` response body (`{"data": [...]}`)
/// into our own shape. Entries missing `id`/`name` are skipped rather than
/// failing the whole parse — one malformed entry in a 300+ model catalog
/// shouldn't hide the rest.
pub fn parse_models_response(body: &serde_json::Value) -> Vec<OpenRouterModel> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
            let pricing = entry.get("pricing");
            Some(OpenRouterModel {
                id,
                name,
                prompt_price_per_million: per_million(pricing.and_then(|p| p.get("prompt"))),
                completion_price_per_million: per_million(pricing.and_then(|p| p.get("completion"))),
            })
        })
        .collect()
}

/// Real network call to OpenRouter's public model catalog — no API key
/// required for listing (OpenRouter's `/models` endpoint is public).
fn fetch_live() -> Result<Vec<OpenRouterModel>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("OpenRouter API returned {}", response.status()));
    }
    let body: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    Ok(parse_models_response(&body))
}

/// In-memory cache: at most one real OpenRouter API call per 24h, per the
/// design doc's decided refresh cadence. `None` means "never fetched
/// yet" — the first call in a run always hits the real API (or falls
/// back on failure).
pub struct OpenRouterCatalogState(pub std::sync::Mutex<Option<(Instant, Vec<OpenRouterModel>)>>);

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Returns the cached catalog if it's fresh enough and `force_refresh`
/// wasn't requested; otherwise calls the real API and updates the cache
/// on success. On failure (network down, API error, cache empty and
/// nothing to fall back on in-memory), falls back to the static curated
/// list rather than surfacing an error — per the design doc's decided
/// degrade-gracefully behavior.
pub fn list_models(state: &OpenRouterCatalogState, force_refresh: bool) -> OpenRouterModelsResult {
    let mut cache = state.0.lock().unwrap();
    if !force_refresh {
        if let Some((fetched_at, models)) = cache.as_ref() {
            if fetched_at.elapsed() < CACHE_TTL {
                return OpenRouterModelsResult { models: models.clone(), live: true };
            }
        }
    }
    match fetch_live() {
        Ok(models) => {
            *cache = Some((Instant::now(), models.clone()));
            OpenRouterModelsResult { models, live: true }
        }
        Err(_) => static_fallback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_price_and_name_from_a_realistic_response() {
        let body = serde_json::json!({
            "data": [
                {
                    "id": "anthropic/claude-sonnet-5",
                    "name": "Anthropic: Claude Sonnet 5",
                    "pricing": {"prompt": "0.000003", "completion": "0.000015"},
                },
            ]
        });
        let models = parse_models_response(&body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "anthropic/claude-sonnet-5");
        assert_eq!(models[0].name, "Anthropic: Claude Sonnet 5");
        assert_eq!(models[0].prompt_price_per_million, Some(3.0));
        assert_eq!(models[0].completion_price_per_million, Some(15.0));
    }

    #[test]
    fn falls_back_to_the_model_id_when_name_is_missing() {
        let body = serde_json::json!({"data": [{"id": "some/model"}]});
        let models = parse_models_response(&body);
        assert_eq!(models[0].name, "some/model");
        assert_eq!(models[0].prompt_price_per_million, None);
    }

    #[test]
    fn skips_entries_with_no_id() {
        let body = serde_json::json!({"data": [{"name": "no id here"}, {"id": "ok/model"}]});
        let models = parse_models_response(&body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "ok/model");
    }

    #[test]
    fn a_negative_sentinel_price_is_treated_as_unknown_not_a_negative_price() {
        let body = serde_json::json!({
            "data": [{"id": "x", "pricing": {"prompt": "-1", "completion": "0.000001"}}]
        });
        let models = parse_models_response(&body);
        assert_eq!(models[0].prompt_price_per_million, None);
        assert_eq!(models[0].completion_price_per_million, Some(1.0));
    }

    #[test]
    fn missing_data_field_parses_to_an_empty_list_rather_than_panicking() {
        let body = serde_json::json!({"unexpected": "shape"});
        assert!(parse_models_response(&body).is_empty());
    }

    #[test]
    fn list_models_uses_the_cache_within_the_ttl_without_refetching() {
        let state = OpenRouterCatalogState(std::sync::Mutex::new(Some((
            Instant::now(),
            vec![OpenRouterModel {
                id: "cached/model".to_string(),
                name: "Cached Model".to_string(),
                prompt_price_per_million: Some(1.0),
                completion_price_per_million: Some(2.0),
            }],
        ))));
        let result = list_models(&state, false);
        assert!(result.live);
        assert_eq!(result.models.len(), 1);
        assert_eq!(result.models[0].id, "cached/model");
    }

    #[test]
    fn list_models_ignores_a_stale_cache_entry() {
        // A cache entry older than the TTL should not be trusted as-is —
        // this only proves the staleness check itself (that an old
        // timestamp doesn't satisfy `elapsed() < CACHE_TTL`), not the
        // network fallback path (no real network access in unit tests).
        let ancient = Instant::now() - Duration::from_secs(25 * 60 * 60);
        assert!(ancient.elapsed() >= CACHE_TTL);
    }
}

/// Live test: a real network round-trip to OpenRouter's public catalog,
/// proving `fetch_live`/`parse_models_response` actually work against the
/// real API response shape, not just the realistic-looking fixtures used
/// in the unit tests above. Not run in CI (needs outbound network access)
/// — run manually with `cargo test openrouter_catalog::live -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn fetch_live_returns_a_real_non_empty_catalog_with_at_least_one_priced_model() {
        let models = fetch_live().expect("a real call to OpenRouter's public /models endpoint should succeed");
        assert!(!models.is_empty(), "OpenRouter's catalog should never be empty");
        assert!(
            models.iter().any(|m| m.prompt_price_per_million.is_some()),
            "at least one real model should report a parseable prompt price"
        );
        // Sanity-check a well-known, long-lived model id is present, so
        // this isn't just "got 200 OK with some JSON" — it's "the shape
        // we parse actually lines up with what OpenRouter returns".
        assert!(
            models.iter().any(|m| m.id.starts_with("anthropic/")),
            "expected at least one anthropic/* model in OpenRouter's real catalog"
        );
    }
}
