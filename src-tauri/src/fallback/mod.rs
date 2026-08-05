//! Fallback: local-first, cloud-backup provider retry chain. Surfaces a
//! coded error (see Error Code Registry) when every provider fails.
//! Design: `Multi-AI Agent Panel Document/06 Resources/Error Code Registry.md`
//!
//! This is a generic "try each candidate in order, wrap the terminal
//! failure as E3001" combinator — deliberately generic over what a
//! "candidate" is (a Key Vault entry, a local endpoint, whatever) so the
//! retry/fallback logic itself is unit-testable without any network
//! access. `agent_manager` supplies the real candidates and the real
//! network-calling closure; see its tests/live tests for the network side.

use crate::agent_manager::providers::ProviderError;

/// Tries `attempt` against each of `candidates` in order, stopping at the
/// first success. If `candidates` is empty, or every attempt fails,
/// returns `ProviderError::AllProvidersFailed` (Error Code Registry E3001)
/// carrying a human-readable description of what was tried and why each
/// one failed.
///
/// `on_attempt` is called once per candidate actually tried, with whether
/// that specific attempt succeeded — this is what lets a caller log each
/// attempt in a fallback chain individually (e.g. into `usage_log`, one row
/// per key tried) instead of only the chain's final outcome. It is not
/// called for candidates never reached (e.g. after an earlier success).
pub fn run_with_fallback<T>(
    candidates: &[T],
    describe: impl Fn(&T) -> String,
    mut attempt: impl FnMut(&T) -> Result<String, ProviderError>,
    mut on_attempt: impl FnMut(&T, bool),
) -> Result<String, ProviderError> {
    if candidates.is_empty() {
        return Err(ProviderError::AllProvidersFailed {
            error_code: "E3001",
            attempts: vec!["no provider/key configured".to_string()],
        });
    }

    let mut attempts_log = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        match attempt(candidate) {
            Ok(reply) => {
                on_attempt(candidate, true);
                return Ok(reply);
            }
            Err(err) => {
                on_attempt(candidate, false);
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
                    Err(ProviderError::Network("should not be reached".to_string()))
                }
            },
            |_, _| {},
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
                    Err(ProviderError::Network("timed out".to_string()))
                } else {
                    Ok("ok from b".to_string())
                }
            },
            |_, _| {},
        );
        assert_eq!(result, Ok("ok from b".to_string()));
    }

    #[test]
    fn wraps_the_terminal_failure_as_e3001_when_every_candidate_fails() {
        let candidates = vec!["a", "b"];
        let err = run_with_fallback(
            &candidates,
            |c| format!("candidate {c}"),
            |_| Err(ProviderError::Network("unreachable".to_string())),
            |_, _| {},
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
        let err = run_with_fallback(&candidates, |c| c.to_string(), |_| unreachable!(), |_, _| {}).unwrap_err();
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
                    Err(ProviderError::Network("nope".to_string()))
                }
            },
            |c, success| log.push((c, success)),
        );
        assert_eq!(result, Ok("ok from b".to_string()));
        // "c" is never reached because "b" already succeeded.
        assert_eq!(log, vec![("a", false), ("b", true)]);
    }
}
