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

/// Windows only: `CREATE_NO_WINDOW` — spawning a console-subsystem
/// executable (`python.exe`) from a GUI app pops up a real, empty
/// console/CMD window on Windows unless told not to, since Windows
/// creates a new console for a child process by default whenever the
/// parent didn't already have a console of its own attached (a GUI
/// subsystem app like this one doesn't). Every bridge subprocess this
/// app spawns (`skill_manager`'s Skills bridge, `ml_engine`'s ML Engine
/// bridge, `game_agent`'s Track B recorder) is a long-running background
/// process the user never interacts with directly — there's no reason
/// for any of them to have a visible console at all, and two of them
/// starting simultaneously at app launch is exactly what produced the
/// "opening the App opens two CMD windows" report this fixes. A no-op
/// on macOS/Linux, where child processes never get their own window in
/// the first place.
pub(crate) fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The relative path `find_bundled_python` looks for under a resource
    /// dir, for whichever platform these tests are actually compiled and
    /// run on — mirrors the per-OS branch under test rather than
    /// hardcoding one platform, so this suite is meaningful in CI on all
    /// three OSes, not just whichever one a developer happens to be on.
    fn expected_relative_path() -> PathBuf {
        if cfg!(target_os = "windows") {
            PathBuf::from("python-windows").join("python.exe")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("python-macos").join("bin").join("python3")
        } else {
            PathBuf::from("python-linux").join("bin").join("python3")
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bridge-support-test-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_bundled_python_returns_none_when_no_resource_dir_is_given() {
        assert_eq!(find_bundled_python(None), None);
    }

    #[test]
    fn find_bundled_python_returns_none_when_the_expected_file_is_missing() {
        let dir = temp_dir("missing");
        assert_eq!(find_bundled_python(Some(&dir)), None);
    }

    #[test]
    fn find_bundled_python_finds_the_interpreter_when_it_exists() {
        let dir = temp_dir("present");
        let relative = expected_relative_path();
        let full_path = dir.join(&relative);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(&full_path, b"").unwrap();

        let found = find_bundled_python(Some(&dir)).expect("should find the interpreter that's actually there");
        assert_eq!(PathBuf::from(found), full_path);
    }

    #[test]
    fn hide_console_window_does_not_prevent_the_process_from_actually_running() {
        // Can't headlessly assert "no window appeared" (there's no
        // automated signal for that), but this proves the flag doesn't
        // break spawning/output capture — a real regression the flag
        // could plausibly cause if it were the wrong constant or applied
        // wrong. `cmd /C echo` prints something recognizable to stdout
        // on Windows; harmlessly a no-op process on other platforms
        // where the function itself is a no-op too.
        let mut command = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/C").arg("echo").arg("hidden-console-smoke-test");
            c
        } else {
            let mut c = Command::new("echo");
            c.arg("hidden-console-smoke-test");
            c
        };
        hide_console_window(&mut command);
        let output = command.output().expect("the process should still spawn and run with the flag applied");
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hidden-console-smoke-test"));
    }

    #[test]
    fn free_local_port_returns_a_port_that_is_actually_bindable() {
        let port = free_local_port().expect("OS should hand back a free port");
        // If the port weren't actually free, this second bind would fail —
        // proves free_local_port() doesn't just return a hardcoded/stale
        // value, it round-trips through the OS.
        let listener = std::net::TcpListener::bind(("127.0.0.1", port));
        assert!(listener.is_ok());
    }
}
