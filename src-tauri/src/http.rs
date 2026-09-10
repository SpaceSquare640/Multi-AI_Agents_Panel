//! Shared HTTP clients, with timeouts chosen per kind of request.
//!
//! # Why this module exists
//!
//! `reqwest::blocking::Client::new()` is **not** the blocking twin of
//! `reqwest::Client::new()`. The async client has no request timeout by
//! default; the blocking one defaults to **30 seconds for the whole
//! request**, including reading the response body. Every `Client::new()`
//! in this crate silently inherited that 30-second budget.
//!
//! Thirty seconds is far too short for inference. Measured on an RTX 4060
//! with `tinyllama` (0.64 GB), the smallest model on the machine:
//!
//! ```text
//! curl, same request body:  37.1 s  -> 200 OK
//! Ollama's own log:         36.8 s  -> POST /api/chat 200
//!   load_duration                     16.1 s
//!   prompt_eval_duration              17.6 s
//!   generation                         3.1 s
//! this crate's adapter:     30.7 s  -> Err(Network { error_code: "E1001" })
//! ```
//!
//! So the first message to a local model failed, and the error told the
//! user the local service could not be reached — a service that was
//! running and had answered correctly. Raising the timeout made the same
//! test pass, which is how the diagnosis was confirmed rather than assumed.
//!
//! The 30-second default was equally wrong in the other direction: the
//! Ollama installer is 1.57 GB (`Content-Length: 1573069568`), and
//! finishing that inside 30 seconds needs a sustained 420 Mbit/s. The
//! "install Ollama from the app" button could not have worked on an
//! ordinary connection.
//!
//! # Why these numbers
//!
//! Split into a short **connect** timeout and a long **overall** one.
//! Unreachable should fail fast; slow should be allowed to be slow. A
//! single overall timeout cannot express both, which is what made
//! "unreachable" and "still thinking" indistinguishable in the first place.
//!
//! Every budget below is a deliberate ceiling, not a guess at how long
//! things take. The point is to bound the wait, not to predict it.

use std::time::Duration;

use reqwest::blocking::Client;

/// Falls back to the default client if a builder ever fails, so a
/// misconfiguration degrades to the old behaviour rather than taking the
/// whole call site down. `build()` only fails on TLS backend
/// initialisation, which would break the default client too — this is
/// belt-and-braces, not an expected path.
fn build(connect: Duration, overall: Option<Duration>) -> Client {
    let mut b = Client::builder().connect_timeout(connect);
    b = match overall {
        Some(t) => b.timeout(t),
        // Explicitly no overall timeout. `Client::builder()` inherits the
        // same 30-second default as `Client::new()`, so this has to be
        // stated rather than left out.
        None => b.timeout(None),
    };
    b.build().unwrap_or_else(|_| Client::new())
}

/// Local inference: Ollama, colibrì, OmniRoute chat calls.
///
/// Five minutes overall. A cold load of a large local model on a modest
/// GPU genuinely can take minutes, and the user chose a local model
/// knowing it is slower — cutting it off is worse than making it wait.
/// Connect stays short because "Ollama is not running" is the common
/// failure and should be reported in seconds.
pub fn local_inference() -> Client {
    build(Duration::from_secs(5), Some(Duration::from_secs(300)))
}

/// Cloud inference: Anthropic, OpenAI, OpenRouter chat calls.
///
/// Three minutes. Long enough for a reasoning model on a long prompt,
/// short enough that a hung provider does not hold a session open
/// indefinitely. Cloud has no cold-load phase, so it needs less headroom
/// than local.
pub fn cloud_inference() -> Client {
    build(Duration::from_secs(10), Some(Duration::from_secs(180)))
}

/// The guardrail classifier, which runs before every outgoing message.
///
/// Deliberately tighter than `local_inference`: this call sits between
/// the user pressing send and anything happening, so its ceiling is the
/// app's own responsiveness. `screen_with_llama_guard` fails open by
/// design, so exceeding this skips the second-pass check rather than
/// blocking the message — a documented trade-off, and the reason a
/// five-minute budget would be the wrong choice here.
///
/// A cold Llama Guard model may exceed 60 seconds on its first call and
/// be skipped. That is stated rather than hidden: the mandatory keyword
/// screen still runs, and the next message will find the model warm.
pub fn guardrail_classifier() -> Client {
    build(Duration::from_secs(5), Some(Duration::from_secs(60)))
}

/// Metadata: model lists, release checks, catalogue fetches.
///
/// Fifteen seconds. These are small JSON reads that either answer
/// promptly or are not worth waiting for — none of them blocks the user
/// from doing something else.
pub fn metadata() -> Client {
    build(Duration::from_secs(5), Some(Duration::from_secs(15)))
}

/// Large downloads: installers and model pulls, measured in gigabytes.
///
/// No overall timeout. A 1.57 GB installer on a slow connection is not an
/// error, and any ceiling picked here would be a guess about someone
/// else's bandwidth. Progress is visible in the UI, and the user can
/// cancel — which is a better stop condition than a number in this file.
/// Connect stays short so an unreachable host still fails quickly.
pub fn large_download() -> Client {
    build(Duration::from_secs(10), None)
}

/// True when a `reqwest` error was a timeout rather than a failure to
/// reach the host at all.
///
/// The Error Code Registry separates these for local providers — E1001
/// "本地推理服務未啟動" from E1004 "本地推理逾時" — and records that E1004
/// was unimplemented precisely because "reqwest 逾時與連不上是同一種錯誤
/// 路徑". This is what closes that gap. Cloud keeps one code (E2003)
/// because the registry deliberately merges the two cases there.
pub fn is_timeout(e: &reqwest::Error) -> bool {
    e.is_timeout()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression this module exists to prevent. If a future change
    /// drops the explicit `timeout`, these clients would silently return
    /// to the 30-second default and the same bug would come back with no
    /// visible symptom other than inference failing on slow machines.
    ///
    /// There is no public getter for a client's configured timeout, so
    /// this asserts what can be asserted: that every constructor returns
    /// a client at all, and that the builders used above are accepted by
    /// this version of reqwest.
    #[test]
    fn every_client_builds() {
        let _ = local_inference();
        let _ = cloud_inference();
        let _ = guardrail_classifier();
        let _ = metadata();
        let _ = large_download();
    }

    #[test]
    fn no_overall_timeout_is_accepted_by_the_builder() {
        // `timeout(None)` is the part that would break first if reqwest
        // ever changed its builder API, and it is the one setting that
        // cannot be expressed by simply omitting a call.
        let _ = build(Duration::from_secs(1), None);
    }
}
