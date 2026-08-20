//! Small helpers shared by the app's Python-subprocess bridges
//! (`skill_manager`, `ml_engine`, and `game_agent`'s Track B tooling):
//! finding a working Python interpreter, and picking a free localhost
//! port for the bridge's own HTTP server to bind. Both were duplicated
//! byte-for-byte across those modules before being consolidated here.
//!
//! **Known gap**: CI-CD Pipeline.md decided the shipped installers
//! should embed a portable Python runtime per platform, so end users
//! never need their own Python (`不依賴使用者系統既有 Python`).
//! `find_bundled_python` below is the first slice of that: on Windows
//! only, it looks for the official embeddable-Python distribution
//! vendored alongside the Skills bridge resources (see
//! `tauri.windows.conf.json`), and callers fall back to `find_python`
//! (searching `PATH`) when it isn't there — e.g. `cargo tauri dev`,
//! where nothing has been bundled. macOS/Linux and the ML Engine
//! bridge (which additionally needs `sentence-transformers`/torch,
//! a much bigger vendoring job) are still unaddressed; they keep
//! relying on the user's own system Python.

use std::path::Path;
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

/// Looks for the bundled Windows embeddable-Python interpreter under
/// `<resource_dir>/python-windows/python.exe` (see
/// `tauri.windows.conf.json`'s `bundle.resources`). Returns its full
/// path as a string if found, so callers can `Command::new` it
/// directly without depending on `PATH`. Always `None` on non-Windows
/// targets and whenever `resource_dir` is `None` (e.g. `cargo tauri
/// dev`, where bundle resources aren't copied anywhere).
pub(crate) fn find_bundled_python(resource_dir: Option<&Path>) -> Option<String> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let dir = resource_dir?;
    let candidate = dir.join("python-windows").join("python.exe");
    if candidate.exists() {
        candidate.to_str().map(str::to_string)
    } else {
        None
    }
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
