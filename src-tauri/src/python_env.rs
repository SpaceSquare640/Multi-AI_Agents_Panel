//! Finds a working Python interpreter on the system `PATH`.
//!
//! Was duplicated byte-for-byte across `game_agent`, `ml_engine`, and
//! `skill_manager` — each needed the same "which Python binary actually
//! works" check for its own subprocess bridge, and grew its own copy.
//! Consolidated here so there's one place to change if the candidate
//! list or detection strategy ever needs to.
//!
//! **Known gap**: CI-CD Pipeline.md decided the shipped installers
//! should embed a portable Python runtime per platform, so end users
//! never need their own Python (`不依賴使用者系統既有 Python`). That
//! was never implemented — `tauri.conf.json`'s `bundle.resources` only
//! ships the `skills/`/`ml/` script directories, not an interpreter,
//! and this function still just searches whatever's on the user's
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
