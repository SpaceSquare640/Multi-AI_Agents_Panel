//! Cancelling a send that is already in flight — the backing for the
//! design's Stop button, which could not be ported when Chat was rebuilt
//! because there was nothing behind it.
//!
//! # What "cancel" can and cannot mean here
//!
//! Every provider call in this app goes through `reqwest::blocking`, and
//! a blocking HTTP request cannot be interrupted from outside once it has
//! been issued. So cancelling does **not** abort the connection: the
//! request already on the wire runs to completion in its own thread, and
//! whatever it costs has already been spent.
//!
//! What cancelling does mean is everything the app itself still controls:
//!
//! 1. **No further provider calls.** The key rotation stops trying the
//!    next key, the cross-provider chain stops trying the next provider,
//!    and the tool-calling loop stops taking another round trip. These
//!    are where the time actually goes — a fallback chain can hold the
//!    user for the sum of several timeouts, and a tool conversation for
//!    up to `MAX_ITERATIONS` round trips.
//! 2. **Nothing is written.** The assistant reply is not persisted.
//!
//! Point 2 is the one that makes this worth doing rather than handling it
//! in the UI alone. If the frontend merely stopped waiting, the command
//! would still run to completion and still write the reply, and a message
//! the user had explicitly cancelled would appear in the transcript
//! seconds later — worse than having no Stop button at all.
//!
//! # Why cancellation is not an Error Code Registry code
//!
//! `ProviderError::Cancelled` carries no `error_code`, unlike every other
//! variant. The registry exists to give a user a code to read, look up,
//! and act on; "you pressed Stop" needs none of that — nothing went
//! wrong, there is nothing to diagnose, and offering a code would imply
//! otherwise.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A handle the send path checks between steps. Cloneable and cheap, so
/// it can be handed down through several layers without ceremony.
///
/// `never()` exists so the many callers that have no user-facing Stop
/// button (group-chat turns, meeting summaries, orchestrator DAG nodes)
/// keep a call signature with no cancellation in it, rather than every
/// one of them inventing an always-false flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Option<Arc<AtomicBool>>);

impl CancelToken {
    /// A token that is never cancelled — for paths with no Stop button.
    pub fn never() -> CancelToken {
        CancelToken(None)
    }

    pub fn is_cancelled(&self) -> bool {
        // `Relaxed` is enough: this flag carries no data of its own and
        // guards no other memory. The only question ever asked of it is
        // "has the user pressed Stop by now?", and observing the press
        // one step later than it happened is indistinguishable from the
        // user having pressed it one step later.
        self.0.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed))
    }
}

/// The in-flight sends that can currently be cancelled, keyed by session.
///
/// Keyed by session rather than by some per-call id because that is what
/// the UI can actually name: the Stop button belongs to a session's
/// composer, and the user pressing it means "stop whatever this session
/// is doing", not "stop request #7".
#[derive(Debug, Default)]
pub struct CancelState(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl CancelState {
    /// Registers a send for `session_id` and returns its token.
    ///
    /// Replaces any token already registered for that session. A session
    /// is not supposed to have two sends running at once (the composer
    /// disables itself while sending), but if it somehow does, the newer
    /// send owns the Stop button — and the older token, still held by
    /// the older call, simply stops being reachable from here, which
    /// leaves that call running to completion rather than cancelling it
    /// by surprise.
    pub fn begin(&self, session_id: &str) -> CancelToken {
        let flag = Arc::new(AtomicBool::new(false));
        self.0.lock().unwrap().insert(session_id.to_string(), Arc::clone(&flag));
        CancelToken(Some(flag))
    }

    /// Asks the in-flight send for `session_id` to stop. Returns whether
    /// there was one to ask — `false` is the ordinary outcome of pressing
    /// Stop just as the reply arrives, not an error.
    pub fn request(&self, session_id: &str) -> bool {
        match self.0.lock().unwrap().get(session_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Drops the registration once a send has finished, cancelled or not.
    /// Must run on every exit path, so that a Stop pressed after the
    /// reply already landed cannot set a flag nothing will ever read —
    /// which would otherwise sit there and cancel the *next* send.
    pub fn finish(&self, session_id: &str) {
        self.0.lock().unwrap().remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_never_token_is_never_cancelled() {
        assert!(!CancelToken::never().is_cancelled());
        assert!(!CancelToken::default().is_cancelled());
    }

    #[test]
    fn a_registered_send_sees_the_request_to_stop() {
        let state = CancelState::default();
        let token = state.begin("session-1");
        assert!(!token.is_cancelled());
        assert!(state.request("session-1"));
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancelling_one_session_leaves_another_alone() {
        let state = CancelState::default();
        let one = state.begin("session-1");
        let two = state.begin("session-2");
        state.request("session-1");
        assert!(one.is_cancelled());
        assert!(!two.is_cancelled());
    }

    #[test]
    fn requesting_a_stop_for_a_session_with_nothing_in_flight_reports_that_rather_than_failing() {
        let state = CancelState::default();
        assert!(!state.request("never-started"));
    }

    #[test]
    fn a_stop_arriving_after_the_send_finished_does_not_cancel_the_next_send() {
        // The race this pins down: the reply lands, `finish` runs, and
        // only then does the user's Stop reach the backend. Without the
        // deregistration the flag would still be sitting in the map and
        // the *next* message in that session would be cancelled before
        // it ever called a provider.
        let state = CancelState::default();
        let first = state.begin("session-1");
        state.finish("session-1");
        assert!(!state.request("session-1"));
        assert!(!first.is_cancelled());

        let second = state.begin("session-1");
        assert!(!second.is_cancelled());
    }

    #[test]
    fn a_token_outlives_its_registration_without_panicking() {
        let state = CancelState::default();
        let token = state.begin("session-1");
        state.finish("session-1");
        // The call still holding this token keeps reading it safely; it
        // just can no longer be cancelled from outside.
        assert!(!token.is_cancelled());
    }
}
