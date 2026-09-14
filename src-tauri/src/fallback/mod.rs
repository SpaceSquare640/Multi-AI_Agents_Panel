//! Fallback: local-first, cloud-backup provider retry chain. Surfaces a
//! coded error (see Error Code Registry) when every provider fails.
//!
//! This is a generic "try each candidate in order, wrap the terminal
//! failure as E3001" combinator — deliberately generic over what a
//! "candidate" is (a Key Vault entry, a local endpoint, whatever) so the
//! retry/fallback logic itself is unit-testable without any network
//! access. `agent_manager` supplies the real candidates and the real
//! network-calling closure; see its tests/live tests for the network side.

use crate::agent_manager::providers::ProviderError;
use crate::cancel::CancelToken;

/// Tries `attempt` against each of `candidates` in order, stopping at the
/// first success. If `candidates` is empty, or every attempt fails,
/// returns `ProviderError::AllProvidersFailed` (Error Code Registry E3001)
/// carrying a human-readable description of what was tried and why each
/// one failed.
///
/// `on_attempt` is called once per candidate actually tried, with the
/// actual outcome of that attempt — this is what lets a caller log each
/// attempt in a fallback chain individually (e.g. into `usage_log`, one
/// row per key tried) instead of only the chain's final outcome, and
/// with enough detail to log more than just success/failure (e.g. the
/// token usage an OpenRouter call reported). It is not called for
/// candidates never reached (e.g. after an earlier success).
///
/// `cancel` is checked before each candidate, so pressing Stop during a
/// long chain stops it at the next attempt boundary instead of sitting
/// through every remaining key's timeout. It cannot interrupt an attempt
/// already in flight — see `crate::cancel` for why, and for what
/// cancelling does guarantee.
///
/// Generic over the success type `R` (originally hardcoded to `String`)
/// so a caller that needs more than just the reply text back — e.g. an
/// OpenRouter call that also wants the token-usage numbers alongside the
/// reply, for `usage_log`'s cost-estimation columns — can instantiate
/// this with `R = (String, Option<TokenUsage>)` without this module
/// needing to know anything about token usage itself. Every existing
/// caller already returns a plain `String` from its `attempt` closure,
/// which is just `R = String` — this widening is source-compatible with
/// all of them.
pub fn run_with_fallback<T, R>(
    candidates: &[T],
    describe: impl Fn(&T) -> String,
    mut attempt: impl FnMut(&T) -> Result<R, ProviderError>,
    mut on_attempt: impl FnMut(&T, Result<&R, &ProviderError>),
    cancel: &CancelToken,
) -> Result<R, ProviderError> {
    if candidates.is_empty() {
        return Err(ProviderError::AllProvidersFailed {
            error_code: "E3001",
            attempts: vec!["no provider/key configured".to_string()],
        });
    }

    let mut attempts_log = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        // Checked before the attempt rather than after, so a Stop pressed
        // while the previous candidate was timing out costs nothing more.
        // Returned as `Cancelled` rather than folded into
        // `AllProvidersFailed`: the remaining candidates were never
        // tried, and reporting them as failures would be a lie about
        // what was attempted.
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        match attempt(candidate) {
            Ok(reply) => {
                on_attempt(candidate, Ok(&reply));
                return Ok(reply);
            }
            Err(err) => {
                on_attempt(candidate, Err(&err));
                attempts_log.push(format!("{}: {err}", describe(candidate)));
            }
        }
    }

    Err(ProviderError::AllProvidersFailed {
        error_code: "E3001",
        attempts: attempts_log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_first_success_without_trying_later_candidates() {
        let candidates = vec!["a", "b", "c"];
        let mut tried = Vec::new();
        let result = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |c| {
                tried.push(*c);
                if *c == "a" {
                    Ok("ok from a".to_string())
                } else {
                    Err(ProviderError::Network { error_code: "E2003", message: "should not be reached".to_string() })
                }
            },
            |_, _: Result<&String, &ProviderError>| {},
            &CancelToken::never(),
        );
        assert_eq!(result, Ok("ok from a".to_string()));
        assert_eq!(tried, vec!["a"]);
    }

    #[test]
    fn falls_through_to_the_second_candidate_when_the_first_fails() {
        let candidates = vec!["a", "b"];
        let result = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |c| {
                if *c == "a" {
                    Err(ProviderError::Network { error_code: "E2003", message: "timed out".to_string() })
                } else {
                    Ok("ok from b".to_string())
                }
            },
            |_, _: Result<&String, &ProviderError>| {},
            &CancelToken::never(),
        );
        assert_eq!(result, Ok("ok from b".to_string()));
    }

    #[test]
    fn wraps_the_terminal_failure_as_e3001_when_every_candidate_fails() {
        let candidates = vec!["a", "b"];
        let err = run_with_fallback(
            &candidates,
            |c| format!("candidate {c}"),
            |_| Err::<String, _>(ProviderError::Network { error_code: "E2003", message: "unreachable".to_string() }),
            |_, _: Result<&String, &ProviderError>| {},
            &CancelToken::never(),
        )
        .unwrap_err();

        match err {
            ProviderError::AllProvidersFailed { error_code, attempts } => {
                assert_eq!(error_code, "E3001");
                assert_eq!(attempts.len(), 2);
                assert!(attempts[0].contains("candidate a"));
                assert!(attempts[1].contains("candidate b"));
            }
            other => panic!("expected AllProvidersFailed, got {other:?}"),
        }
    }

    #[test]
    fn no_candidates_at_all_is_also_e3001() {
        let candidates: Vec<&str> = vec![];
        let err = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |_| -> Result<String, _> { unreachable!() },
            |_, _: Result<&String, &ProviderError>| {},
            &CancelToken::never(),
        )
        .unwrap_err();
        assert!(matches!(err, ProviderError::AllProvidersFailed { error_code: "E3001", .. }));
    }

    #[test]
    fn on_attempt_fires_once_per_candidate_actually_tried_with_its_own_outcome() {
        let candidates = vec!["a", "b", "c"];
        let mut log: Vec<(&str, bool)> = Vec::new();
        let result = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |c| {
                if *c == "b" {
                    Ok("ok from b".to_string())
                } else {
                    Err(ProviderError::Network { error_code: "E2003", message: "nope".to_string() })
                }
            },
            |c, outcome: Result<&String, &ProviderError>| log.push((c, outcome.is_ok())),
            &CancelToken::never(),
        );
        assert_eq!(result, Ok("ok from b".to_string()));
        // "c" is never reached because "b" already succeeded.
        assert_eq!(log, vec![("a", false), ("b", true)]);
    }

    #[test]
    fn on_attempt_carries_the_actual_ok_value_not_just_a_success_bool() {
        let candidates = vec!["a"];
        let mut seen: Option<String> = None;
        run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |_| Ok("the real reply".to_string()),
            |_, outcome: Result<&String, &ProviderError>| seen = outcome.ok().cloned(),
            &CancelToken::never(),
        )
        .unwrap();
        assert_eq!(seen, Some("the real reply".to_string()));
    }

    #[test]
    fn a_cancelled_chain_stops_at_the_next_candidate_instead_of_trying_the_rest() {
        let candidates = vec!["a", "b", "c"];
        let mut tried = Vec::new();
        let state = crate::cancel::CancelState::default();
        let token = state.begin("session-1");

        let result = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |c| {
                tried.push(*c);
                // The user presses Stop while the first candidate is
                // failing; "b" and "c" must never be attempted.
                state.request("session-1");
                Err(ProviderError::Network { error_code: "E2003", message: "timed out".to_string() })
            },
            |_, _: Result<&String, &ProviderError>| {},
            &token,
        );

        assert_eq!(result, Err(ProviderError::Cancelled));
        assert_eq!(tried, vec!["a"]);
    }

    #[test]
    fn a_chain_cancelled_before_it_starts_never_calls_a_provider_at_all() {
        let candidates = vec!["a"];
        let state = crate::cancel::CancelState::default();
        let token = state.begin("session-1");
        state.request("session-1");

        let result = run_with_fallback(
            &candidates,
            |c| c.to_string(),
            |_| -> Result<String, _> { panic!("no provider should be called after a cancel") },
            |_, _: Result<&String, &ProviderError>| {},
            &token,
        );
        assert_eq!(result, Err(ProviderError::Cancelled));
    }
}
