//! Track A of the Game-Playing Agent design (see
//! `Multi-AI Agent Panel Document/04 Agents & Orchestration/Game-Playing Agent Design.md`,
//! ADR 0005): a persistent screenshot → vision-model-inference →
//! mouse/keyboard-simulation loop, managed directly by Rust rather than
//! the Skills/ML Engine JSON-RPC pattern (this is a continuous loop, not
//! a request/response call — see ADR 0005 for why that pattern doesn't
//! fit here).
//!
//! This executes *real* mouse/keyboard automation on the user's machine.
//! It never starts itself — `start` only runs when a user explicitly
//! calls the `start_game_agent` command, and `is_running`/`stop` are the
//! only way to check/end it. There is no autonomous trigger anywhere in
//! this module.
//!
//! **Bot-detection risk, stated honestly**: the decision tick and mouse
//! movement both carry small randomized jitter (`jittered_duration`,
//! `mouse_path`) rather than a perfectly constant interval and an
//! instantly-teleporting cursor — those are the *most naive* automation
//! signatures a basic heuristic checks for, so avoiding them is a real,
//! cheap improvement. It is not, and cannot honestly be claimed to be, a
//! defense against real anti-cheat (server-side statistical analysis,
//! kernel-level input monitoring). Track A should not be pointed at a
//! game with server-enforced anti-cheat or a ToS prohibiting automation —
//! see `TICK_INTERVAL`'s doc comment.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use serde::{Deserialize, Serialize};
use xcap::Monitor;

use crate::bridge_support::find_python;

use crate::agent_manager::providers::ollama;

/// One decision the vision model can ask for. Deliberately a closed,
/// explicit set — the model must express its intent as exactly one of
/// these actions, not free-form code or shell commands, so
/// `execute_action` can never run anything beyond a plain click/key/wait.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    Click { x: i32, y: i32 },
    Key { key: String },
    Wait,
}

/// Vision models often wrap their JSON answer in prose or a code fence
/// ("Here's what I see: ```json\n{...}\n```") — this pulls out the first
/// `{...}` block and parses it, rather than requiring the whole reply to
/// be exactly one JSON value. Returns `None` (never panics) for a reply
/// with no parseable action, so an unexpected model response just skips
/// that tick instead of crashing the loop.
pub fn parse_agent_action(text: &str) -> Option<AgentAction> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

/// Maps the small set of key names the prompt asks the model to use onto
/// `enigo::Key` — deliberately not exhaustive of every possible key,
/// just the ones a game-dispatch UI plausibly needs. An unrecognized name
/// is a normal `Err`, not a panic.
fn parse_key(name: &str) -> Option<enigo::Key> {
    match name.to_lowercase().as_str() {
        "space" => Some(enigo::Key::Space),
        "enter" | "return" => Some(enigo::Key::Return),
        "escape" | "esc" => Some(enigo::Key::Escape),
        "tab" => Some(enigo::Key::Tab),
        "backspace" => Some(enigo::Key::Backspace),
        "up" => Some(enigo::Key::UpArrow),
        "down" => Some(enigo::Key::DownArrow),
        "left" => Some(enigo::Key::LeftArrow),
        "right" => Some(enigo::Key::RightArrow),
        other => other.chars().next().filter(|_| other.chars().count() == 1).map(enigo::Key::Unicode),
    }
}

/// Encodes the primary monitor's current frame as a base64 PNG — the
/// exact shape `ollama::send_vision` needs for its `images` field.
fn capture_screenshot_base64() -> Result<String, String> {
    let monitors = Monitor::all().map_err(|e| format!("could not list monitors: {e}"))?;
    let monitor = monitors.first().ok_or("no monitor found")?;
    let image = monitor.capture_image().map_err(|e| format!("could not capture screenshot: {e}"))?;

    let mut png_bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| format!("could not encode screenshot as PNG: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png_bytes))
}

/// How many intermediate points to move the mouse through on the way to
/// a click target, instead of teleporting there in one `move_mouse`
/// call. Chosen as a small, cheap number — this is not motion-curve
/// realism (no easing, no overshoot), just enough that the cursor's
/// position is sampled moving across the screen rather than appearing
/// at its destination with zero travel, which is one of the most naive
/// signals a bot-detection heuristic checks for. See module docs on
/// what this does and doesn't defend against.
const MOUSE_PATH_STEPS: u32 = 6;

/// Delay between each intermediate mouse-movement step — small and
/// jittered (see `jittered_duration`) so the whole path takes on the
/// order of ~100ms total, not a human-realistic multi-hundred-ms
/// movement, but not an instantaneous jump either.
const MOUSE_STEP_BASE_DELAY: Duration = Duration::from_millis(15);

/// Linearly interpolated waypoints from `from` to `to` (inclusive of
/// `to`, exclusive of `from` — the caller is already at `from`), evenly
/// spaced into `steps` segments. Pure and deterministic so it's
/// unit-testable without a real mouse; the small amount of realism this
/// buys (a moving cursor instead of a teleporting one) is deliberately
/// simple — see `MOUSE_PATH_STEPS`'s doc comment for the honest scope.
fn mouse_path(from: (i32, i32), to: (i32, i32), steps: u32) -> Vec<(i32, i32)> {
    let steps = steps.max(1);
    (1..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let x = from.0 as f64 + (to.0 - from.0) as f64 * t;
            let y = from.1 as f64 + (to.1 - from.1) as f64 * t;
            (x.round() as i32, y.round() as i32)
        })
        .collect()
}

/// Applies up to ±`jitter_fraction` random variation to `base`, using
/// `random_unit` (expected in `[-1.0, 1.0]`) as the source of
/// randomness — pulled out as a parameter rather than calling `rand`
/// directly so the jitter math itself is unit-testable without
/// depending on actual randomness. A perfectly constant, unvarying
/// interval between actions (identical delay every single time, down to
/// the millisecond) is itself a detectable automation signature, not
/// just how fast the actions happen — see module docs.
fn jittered_duration(base: Duration, jitter_fraction: f64, random_unit: f64) -> Duration {
    let factor = 1.0 + jitter_fraction * random_unit.clamp(-1.0, 1.0);
    Duration::from_secs_f64((base.as_secs_f64() * factor).max(0.0))
}

fn execute_action(enigo: &mut Enigo, action: &AgentAction) -> Result<(), String> {
    match action {
        AgentAction::Click { x, y } => {
            let from = enigo.location().map_err(|e| e.to_string())?;
            for (step_x, step_y) in mouse_path(from, (*x, *y), MOUSE_PATH_STEPS) {
                enigo.move_mouse(step_x, step_y, Coordinate::Abs).map_err(|e| e.to_string())?;
                std::thread::sleep(jittered_duration(MOUSE_STEP_BASE_DELAY, 0.5, rand::random::<f64>() * 2.0 - 1.0));
            }
            enigo.button(Button::Left, Direction::Click).map_err(|e| e.to_string())
        }
        AgentAction::Key { key } => {
            let k = parse_key(key).ok_or_else(|| format!("unrecognized key \"{key}\""))?;
            enigo.key(k, Direction::Click).map_err(|e| e.to_string())
        }
        AgentAction::Wait => Ok(()),
    }
}

/// Base time to sleep between decision ticks — a real, randomized
/// interval is derived from this (see `jittered_duration`, used in the
/// loop in `start`), not this fixed value directly. Deliberately not
/// configurable yet (see Backlog follow-up), just a conservative pace so
/// a misbehaving loop can't hammer the local Ollama instance or spam
/// mouse clicks faster than a human could react to stop it.
///
/// **On bot detection, stated honestly**: the timing jitter and
/// multi-step mouse movement in this module reduce the *most naive*
/// automation signatures (perfectly periodic timing, instantly
/// teleporting cursor positions) — they are not, and cannot honestly be
/// claimed to be, a defense against real anti-cheat systems (server-side
/// statistical analysis, kernel-level input monitoring, behavioral
/// fingerprinting). Track A should not be pointed at any game with
/// server-enforced anti-cheat or a ToS prohibiting automation — that's a
/// real risk (account bans, ToS violations) this module cannot
/// engineer away, only a choice of which games to run it against can.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Shared running flag — the only thing `start`/`stop`/`is_running` (and
/// therefore the `start_game_agent`/`stop_game_agent`/`game_agent_status`
/// commands) coordinate through. Managed as Tauri state, initialized to
/// `false` at app startup (see `lib.rs`).
pub struct GameAgentState(pub Arc<AtomicBool>);

/// Starts the vision loop on a background thread. Returns an error
/// immediately (does not spawn a second loop) if one is already running —
/// there is exactly one game-agent loop at a time, never a "fire another
/// one" case that could end up with two threads both moving the mouse.
pub fn start(state: &GameAgentState, model: String, prompt: String) -> Result<(), String> {
    if state.0.swap(true, Ordering::SeqCst) {
        return Err("game agent is already running".to_string());
    }
    let running = Arc::clone(&state.0);
    std::thread::spawn(move || {
        let mut enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("game_agent: failed to initialize input simulation: {e}");
                running.store(false, Ordering::SeqCst);
                return;
            }
        };
        while running.load(Ordering::SeqCst) {
            match capture_screenshot_base64() {
                Ok(image_b64) => match ollama::send_vision(&model, &prompt, &image_b64) {
                    Ok(reply) => {
                        if let Some(action) = parse_agent_action(&reply) {
                            if let Err(e) = execute_action(&mut enigo, &action) {
                                eprintln!("game_agent: action failed: {e}");
                            }
                        }
                        // A reply with no parseable action is treated as
                        // "the model chose not to act this tick" —
                        // logged nowhere, not an error, since a vision
                        // model narrating instead of answering in JSON
                        // is an expected, recoverable outcome, not a bug.
                    }
                    Err(e) => eprintln!("game_agent: vision call failed: {e}"),
                },
                Err(e) => eprintln!("game_agent: screenshot failed: {e}"),
            }
            std::thread::sleep(jittered_duration(TICK_INTERVAL, 0.3, rand::random::<f64>() * 2.0 - 1.0));
        }
    });
    Ok(())
}

/// Signals the loop to stop after its current tick — not an immediate
/// kill, since the loop only checks `running` between ticks (see `start`).
/// The longest a stop can take to take effect is one screenshot + one
/// model call + one action, not indefinite.
pub fn stop(state: &GameAgentState) {
    state.0.store(false, Ordering::SeqCst);
}

pub fn is_running(state: &GameAgentState) -> bool {
    state.0.load(Ordering::SeqCst)
}

// --- Track B (Deep RL): the `record` pipeline stage ---
//
// Everything below manages `game_agent_rl`'s Python CLI as a background
// subprocess — the "record" stage of the design doc's §4 pipeline
// (record → label → train-bc → train-rl → play). `record`/`label`/
// `train-bc` all exist on the Python side now (`game_agent_rl/record.py`,
// `game_agent_rl/label.py`, `game_agent_rl/train_bc.py`) — `train-rl`/
// `play` are still future work, not stubbed here. Only `record` is wired
// into Rust below: it's the one long-running background job that needs a
// start/stop lifecycle Rust manages. `label`/`train-bc` are short(-ish),
// one-shot CLI invocations over an already-recorded/labeled session
// directory (`python -m game_agent_rl.cli label --session-dir <dir>`,
// `... train-bc --session-dir <dir> --checkpoint-out <path>`) — a
// developer/researcher runs them manually once real demonstration data
// exists, the same "run this yourself" posture as the rest of
// `game_agent_rl` (see `resolve_recording_dir`'s doc comment on why this
// directory isn't bundled). Per ADR 0005, `record` is a standalone CLI
// tool Rust starts/monitors/stops — not the JSON-RPC bridge pattern
// `skill_manager`/`ml_engine` use, since a recording session is a
// long-running background job, not a request/response call.

/// Resolves the `game_agent_rl/` directory: the packaged resource
/// location first, falling back to the repo-relative path for `cargo
/// tauri dev` — same pattern as `skill_manager::resolve_skills_dir`/
/// `ml_engine::resolve_ml_dir`. Unlike those, `game_agent_rl` is
/// deliberately *not* added to `tauri.conf.json`'s `bundle.resources` —
/// see its `requirements.txt`: this is developer/research tooling run
/// manually, not a capability the packaged app calls at runtime.
pub fn resolve_recording_dir(resource_dir: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = resource_dir {
        let candidate = dir.join("game_agent_rl");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("game_agent_rl")
}

/// Owns the recording subprocess's `Child` handle, if one is running.
/// `None` means no session is currently being recorded.
pub struct RecordingState(pub std::sync::Mutex<Option<std::process::Child>>);

/// Starts `python -m game_agent_rl.cli record` as a background
/// subprocess, writing frames/events under `output_dir/session`.
/// Returns an error immediately (does not spawn a second recorder) if a
/// session is already running.
pub fn start_recording(
    state: &RecordingState,
    game_agent_rl_dir: &std::path::Path,
    session: &str,
    output_dir: &str,
) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    if guard.is_some() {
        return Err("a recording session is already running".to_string());
    }
    let python_bin = find_python().ok_or("no working Python interpreter found on PATH")?;
    let working_dir = game_agent_rl_dir
        .parent()
        .ok_or_else(|| format!("could not resolve the parent of {game_agent_rl_dir:?}"))?;
    let mut command = Command::new(&python_bin);
    crate::bridge_support::hide_console_window(&mut command);
    let child = command
        .arg("-m")
        .arg("game_agent_rl.cli")
        .arg("record")
        .arg("--session")
        .arg(session)
        .arg("--output-dir")
        .arg(output_dir)
        .current_dir(working_dir)
        .spawn()
        .map_err(|e| format!("failed to spawn {python_bin} -m game_agent_rl.cli record: {e}"))?;
    *guard = Some(child);
    Ok(())
}

/// Stops the running recording session. This is a hard process
/// termination (`Child::kill`), not the graceful `Ctrl+C`/`SIGINT` the
/// CLI's own `try/except KeyboardInterrupt` is written to handle — a
/// real terminal Ctrl+C isn't reproducible by signaling a child process
/// this way on every platform. Frames/events already written to disk
/// are unaffected either way; only the "N frames recorded" summary the
/// CLI prints on a graceful stop is skipped.
pub fn stop_recording(state: &RecordingState) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let Some(mut child) = guard.take() else {
        return Err("no recording session is running".to_string());
    };
    child.kill().map_err(|e| e.to_string())?;
    let _ = child.wait();
    Ok(())
}

pub fn is_recording(state: &RecordingState) -> bool {
    state.0.lock().unwrap().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_path_ends_exactly_at_the_target() {
        let path = mouse_path((0, 0), (100, 50), 6);
        assert_eq!(path.last(), Some(&(100, 50)));
    }

    #[test]
    fn mouse_path_has_one_point_per_step() {
        let path = mouse_path((0, 0), (100, 50), 6);
        assert_eq!(path.len(), 6);
    }

    #[test]
    fn mouse_path_moves_monotonically_toward_the_target_on_each_axis() {
        let path = mouse_path((0, 0), (100, -50), 5);
        let mut prev = (0, 0);
        for point in &path {
            assert!(point.0 >= prev.0, "x should never move backward toward a positive target");
            assert!(point.1 <= prev.1, "y should never move backward toward a negative target");
            prev = *point;
        }
    }

    #[test]
    fn mouse_path_handles_a_zero_distance_click_without_panicking() {
        let path = mouse_path((10, 10), (10, 10), 6);
        assert!(path.iter().all(|&p| p == (10, 10)));
    }

    #[test]
    fn mouse_path_clamps_a_zero_step_count_to_at_least_one() {
        let path = mouse_path((0, 0), (10, 10), 0);
        assert_eq!(path, vec![(10, 10)]);
    }

    #[test]
    fn jittered_duration_with_zero_randomness_equals_the_base() {
        let base = Duration::from_millis(1000);
        assert_eq!(jittered_duration(base, 0.3, 0.0), base);
    }

    #[test]
    fn jittered_duration_at_the_positive_extreme_applies_the_full_jitter_fraction() {
        let base = Duration::from_millis(1000);
        let jittered = jittered_duration(base, 0.3, 1.0);
        assert_eq!(jittered, Duration::from_millis(1300));
    }

    #[test]
    fn jittered_duration_at_the_negative_extreme_applies_the_full_jitter_fraction() {
        let base = Duration::from_millis(1000);
        let jittered = jittered_duration(base, 0.3, -1.0);
        assert_eq!(jittered, Duration::from_millis(700));
    }

    #[test]
    fn jittered_duration_never_goes_negative_even_with_a_jitter_fraction_over_one() {
        let base = Duration::from_millis(100);
        let jittered = jittered_duration(base, 2.0, -1.0);
        assert!(jittered.as_secs_f64() >= 0.0);
    }

    #[test]
    fn jittered_duration_clamps_random_input_outside_the_expected_unit_range() {
        let base = Duration::from_millis(1000);
        // A caller passing something outside [-1.0, 1.0] (a bug in the
        // caller, not this function) shouldn't produce a wilder result
        // than the documented ±jitter_fraction bound.
        assert_eq!(jittered_duration(base, 0.3, 5.0), jittered_duration(base, 0.3, 1.0));
        assert_eq!(jittered_duration(base, 0.3, -5.0), jittered_duration(base, 0.3, -1.0));
    }

    #[test]
    fn parse_agent_action_reads_a_click() {
        let action = parse_agent_action(r#"{"action":"click","x":100,"y":200}"#).unwrap();
        assert_eq!(action, AgentAction::Click { x: 100, y: 200 });
    }

    #[test]
    fn parse_agent_action_reads_a_key() {
        let action = parse_agent_action(r#"{"action":"key","key":"space"}"#).unwrap();
        assert_eq!(action, AgentAction::Key { key: "space".to_string() });
    }

    #[test]
    fn parse_agent_action_reads_wait() {
        let action = parse_agent_action(r#"{"action":"wait"}"#).unwrap();
        assert_eq!(action, AgentAction::Wait);
    }

    #[test]
    fn parse_agent_action_extracts_json_from_surrounding_prose() {
        let text = "Looking at the screen, I should click here.\n```json\n{\"action\":\"click\",\"x\":42,\"y\":7}\n```\nThat should work.";
        let action = parse_agent_action(text).unwrap();
        assert_eq!(action, AgentAction::Click { x: 42, y: 7 });
    }

    #[test]
    fn parse_agent_action_returns_none_for_prose_with_no_json_at_all() {
        assert!(parse_agent_action("I think we should wait and see.").is_none());
    }

    #[test]
    fn parse_agent_action_returns_none_for_an_unknown_action_name() {
        assert!(parse_agent_action(r#"{"action":"launch_missiles"}"#).is_none());
    }

    #[test]
    fn parse_key_recognizes_named_keys_and_single_characters() {
        assert_eq!(parse_key("space"), Some(enigo::Key::Space));
        assert_eq!(parse_key("Enter"), Some(enigo::Key::Return));
        assert_eq!(parse_key("a"), Some(enigo::Key::Unicode('a')));
    }

    #[test]
    fn parse_key_returns_none_for_multi_character_garbage() {
        assert_eq!(parse_key("notakey"), None);
    }

    #[test]
    fn start_reports_an_error_rather_than_double_spawning_when_already_running() {
        let state = GameAgentState(Arc::new(AtomicBool::new(false)));
        // Simulate "already running" without actually spawning a real
        // capture/input thread in a unit test.
        state.0.store(true, Ordering::SeqCst);
        let err = start(&state, "llava".to_string(), "look".to_string()).unwrap_err();
        assert!(err.contains("already running"));
    }

    #[test]
    fn stop_and_is_running_round_trip() {
        let state = GameAgentState(Arc::new(AtomicBool::new(true)));
        assert!(is_running(&state));
        stop(&state);
        assert!(!is_running(&state));
    }

    #[test]
    fn resolve_recording_dir_falls_back_to_the_repo_relative_path_when_no_resource_dir_is_given() {
        let dir = resolve_recording_dir(None);
        assert!(dir.ends_with("game_agent_rl"));
    }

    #[test]
    fn stop_recording_reports_an_error_when_nothing_is_running() {
        let state = RecordingState(std::sync::Mutex::new(None));
        let err = stop_recording(&state).unwrap_err();
        assert!(err.contains("no recording session"));
    }

    #[test]
    fn is_recording_reflects_whether_a_child_handle_is_held() {
        let state = RecordingState(std::sync::Mutex::new(None));
        assert!(!is_recording(&state));
    }
}
