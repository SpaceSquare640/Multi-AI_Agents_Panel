//! Small helpers shared by the app's Python-subprocess bridges
//! (`skill_manager`, `ml_engine`, and `game_agent`'s Track B tooling):
//! finding a working Python interpreter, and picking a free localhost
//! port for the bridge's own HTTP server to bind. Both were duplicated
//! byte-for-byte across those modules before being consolidated here.
//!
//! **Known gap**: CI-CD Pipeline.md decided the shipped installers
//! should embed a portable Python runtime per platform, so end users
//! never need their own Python (`不依賴使用者系統既有 Python`). That
//! was never implemented — `tauri.conf.json`'s `bundle.resources` only
//! ships the `skills/`/`ml/` script directories, not an interpreter,
//! and `find_python` still just searches whatever's on the user's
//! `PATH`. Bundling a real portable Python is a much larger task
//! (vendoring per-platform builds, installer size, updating every
//! caller to prefer the bundled one) — out of scope here; this module
//! only removes the duplication in what already exists.

use std::process::Command;

pub(crate) fn find_python() -> Option<String> {
    for candidate in ["python", "python3", "py"] {
        let works = Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if works {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Binds port 0 (OS picks any free port) and immediately reads back
/// which one it got — the standard "ask the OS for a free port" trick.
/// There's an inherent TOCTOU gap between this returning and the bridge
/// subprocess actually binding it, but that's the same gap every
/// caller already had before this was deduplicated; not introduced or
/// worsened by consolidating the three copies into one.
pub(crate) fn free_local_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
