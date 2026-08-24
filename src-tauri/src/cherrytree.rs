//! One-click installer for CherryTree (<https://github.com/giuspen/cherrytree>),
//! a standalone hierarchical note-taking app — offered as a companion
//! option next to this app's own native Notes feature (see `storage::Note`)
//! for users who want CherryTree's fuller feature set (rich text, code
//! highlighting, encryption, etc.). This app never embeds or launches
//! CherryTree itself once installed — it's an entirely separate program
//! after this point, same posture as `agent_manager::providers::ollama`'s
//! installer.

/// Finds the current release's Windows installer asset from GitHub's API
/// rather than hardcoding a version-pinned URL — CherryTree's release
/// asset filenames embed the version number (e.g.
/// `cherrytree_1.7.2.0_win64_setup.exe`), so unlike Ollama's stable
/// `OllamaSetup.exe` URL there's no fixed "latest" download link to point
/// at directly. Picks the plain `_win64_setup.exe` asset (LaTeX support
/// included) over the `_nolatex` variant, and over the portable `.7z`
/// archive — an installer is what "install" should mean here.
#[cfg(windows)]
fn latest_windows_installer_url() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }

    let response = reqwest::blocking::Client::new()
        .get("https://api.github.com/repos/giuspen/cherrytree/releases/latest")
        // GitHub's API requires a User-Agent header on every request, or
        // it responds 403 regardless of rate limit.
        .header("User-Agent", "multi-ai-agents-panel")
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("failed to query the latest CherryTree release: {e}"))?;
    let release: Release = response.json().map_err(|e| format!("failed to parse the release response: {e}"))?;

    release
        .assets
        .into_iter()
        .find(|a| a.name.ends_with("_win64_setup.exe") && !a.name.contains("nolatex"))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| "no Windows installer asset found in the latest CherryTree release".to_string())
}

/// Downloads the real CherryTree installer from its official GitHub
/// releases and launches it — the installer's own UI (including any
/// Windows UAC elevation prompt) is what the user actually interacts
/// with from here; this function only fetches it and launches it, it
/// never runs anything silently. Only ever reached after the user clicks
/// an explicit "Install CherryTree" button behind its own confirmation
/// dialog.
#[cfg(windows)]
pub fn download_and_run_installer() -> Result<(), String> {
    let url = latest_windows_installer_url()?;
    let response = reqwest::blocking::get(&url)
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("failed to download the CherryTree installer from {url}: {e}"))?;
    let bytes = response.bytes().map_err(|e| format!("failed to read the downloaded installer: {e}"))?;

    let temp_path = std::env::temp_dir().join("CherryTreeSetup.exe");
    std::fs::write(&temp_path, &bytes).map_err(|e| format!("failed to save the installer to disk: {e}"))?;

    let mut command = std::process::Command::new(&temp_path);
    crate::bridge_support::hide_console_window(&mut command);
    command.spawn().map_err(|e| format!("failed to launch the installer: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn download_and_run_installer() -> Result<(), String> {
    Err("Automatic CherryTree installation is only implemented for Windows — \
         download it yourself from https://www.giuspen.net/cherrytree/#downl"
        .to_string())
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn download_and_run_installer_fails_closed_on_non_windows_rather_than_attempting_anything() {
        let err = download_and_run_installer().unwrap_err();
        assert!(err.contains("giuspen.net"), "expected a pointer to the official download page, got {err}");
    }
}
