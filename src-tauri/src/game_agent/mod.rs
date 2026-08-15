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

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use serde::{Deserialize, Serialize};
use xcap::Monitor;

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

fn execute_action(enigo: &mut Enigo, action: &AgentAction) -> Result<(), String> {
    match action {
        AgentAction::Click { x, y } => {
            enigo.move_mouse(*x, *y, Coordinate::Abs).map_err(|e| e.to_string())?;
            enigo.button(Button::Left, Direction::Click).map_err(|e| e.to_string())
        }
        AgentAction::Key { key } => {
            let k = parse_key(key).ok_or_else(|| format!("unrecognized key \"{key}\""))?;
            enigo.key(k, Direction::Click).map_err(|e| e.to_string())
        }
        AgentAction::Wait => Ok(()),
    }
}

/// How long to sleep between decision ticks — deliberately not
/// configurable yet (see Backlog follow-up), just a fixed, conservative
/// pace so a misbehaving loop can't hammer the local Ollama instance or
/// spam mouse clicks faster than a human could react to stop it.
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
            std::thread::sleep(TICK_INTERVAL);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
