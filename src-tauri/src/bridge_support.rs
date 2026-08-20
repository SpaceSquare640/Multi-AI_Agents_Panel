//! Small helpers shared by the app's Python-subprocess bridges
//! (`skill_manager`, `ml_engine`, and `game_agent`'s Track B tooling):
//! finding a working Python interpreter, and picking a free localhost
//! port for the bridge's own HTTP server to bind. Both were duplicated
//! byte-for-byte across those modules before being consolidated here.
//!
//! **Known gap**: CI-CD Pipeline.md decided the shipped installers
//! should embed a portable Python runtime per platform, so end users
//! never need their own Python (`不依賴使用者系統既有 Python`).
//! `find_bundled_python` below covers the Skills bridge on all three
//! desktop platforms: Windows gets the official embeddable-Python
//! distribution, macOS/Linux get an `install_only` build from
//! `astral-sh/python-build-standalone` (there's no equivalent official
//! portable build from python.org for those platforms). Each is
//! vendored alongside the Skills bridge resources at build time (see
//! `tauri.windows.conf.json`/`tauri.macos.conf.json`/`tauri.linux.conf.json`
//! and `release.yml`'s per-platform download steps), and callers fall
//! back to `find_python` (searching `PATH`) when nothing was bundled —
//! e.g. `cargo tauri dev`, where resources aren't copied anywhere. The
//! ML Engine bridge (which additionally needs `sentence-transformers`/
//! torch, a much bigger vendoring job) is still unaddressed; it keeps
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

/// Looks for the bundled Skills-bridge Python interpreter under the
/// per-platform resource subfolder each of the three
/// `tauri.<platform>.conf.json` files bundles it into: `python-windows/
/// python.exe` on Windows, `python-macos/bin/python3` on macOS,
/// `python-linux/bin/python3` on Linux (the last two are `install_only`
/// layouts from `python-build-standalone`, which puts the interpreter
/// under `bin/`, unlike Windows's flat embeddable package). Returns its
/// full path as a string if found, so callers can `Command::new` it
/// directly without depending on `PATH`. `None` whenever `resource_dir`
/// is `None` (e.g. `cargo tauri dev`) or the expected file isn't there.
pub(crate) fn find_bundled_python(resource_dir: Option<&Path>) -> Option<String> {
    let dir = resource_dir?;
    let candidate = if cfg!(target_os = "windows") {
        dir.join("python-windows").join("python.exe")
    } else if cfg!(target_os = "macos") {
        dir.join("python-macos").join("bin").join("python3")
    } else if cfg!(target_os = "linux") {
        dir.join("python-linux").join("bin").join("python3")
    } else {
        return None;
    };
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
