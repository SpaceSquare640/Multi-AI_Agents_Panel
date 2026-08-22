//! In-app "check for updates" — queries this project's own GitHub
//! Releases (not `tauri-plugin-updater`'s signed-manifest mechanism,
//! which needs code signing this project doesn't have yet — see the
//! vault's CI-CD Pipeline.md, code signing is deferred to the Beta
//! stage). This is deliberately check-only: it tells the user a newer
//! version exists and links to the Release page to download it
//! themselves, it never downloads or installs anything on its own.
//!
//! Uses the plain "list releases" endpoint rather than GitHub's
//! `/releases/latest` convenience endpoint, because that endpoint
//! explicitly excludes prereleases — and every release this project has
//! published so far is tagged `--prerelease` (Alpha stage, per the
//! three-stage version policy in CI-CD Pipeline.md). `/releases/latest`
//! would report "no releases" for this entire project today.

use serde::{Deserialize, Serialize};

const RELEASES_URL: &str = "https://api.github.com/repos/SpaceSquare640/Multi-AI_Agents_Panel/releases?per_page=1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

/// Strips a leading `v` and drops any `-alpha`/`-beta`/etc. prerelease
/// suffix, then parses the remaining `major.minor.patch` as three
/// integers. Returns `None` for anything that doesn't fit that shape
/// rather than guessing — an unparseable version should never be
/// silently treated as "up to date" or "update available."
fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim_start_matches('v').split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Pure comparison, split out from the network call so it's fully
/// unit-testable: `true` only if `latest` parses to a strictly greater
/// `(major, minor, patch)` tuple than `current`. Unparseable input on
/// either side fails closed (`false`, "no update"), not open — a
/// version-check bug should never nag a user who's already current, or
/// worse, mislead them into thinking a broken comparison means they're
/// current when it couldn't actually tell.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
}

/// Fetches the most recently published release (including prereleases)
/// and compares it to `current_version`. GitHub's REST API requires a
/// `User-Agent` header on every request — no API key needed for public
/// unauthenticated reads like this one, but requests without a
/// `User-Agent` are rejected outright.
pub fn check_for_update(current_version: &str) -> Result<UpdateCheckResult, String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(RELEASES_URL)
        .header("User-Agent", "multi-ai-agents-panel-update-check")
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("could not reach GitHub: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned {}", response.status()));
    }

    let releases: Vec<ReleaseResponse> =
        response.json().map_err(|e| format!("could not parse GitHub's response: {e}"))?;
    let latest = releases.first().ok_or_else(|| "no releases found".to_string())?;

    Ok(UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version: latest.tag_name.clone(),
        update_available: is_newer(current_version, &latest.tag_name),
        release_url: latest.html_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_alpha_version() {
        assert_eq!(parse_semver("0.5.0-alpha"), Some((0, 5, 0)));
    }

    #[test]
    fn parses_a_v_prefixed_tag() {
        assert_eq!(parse_semver("v0.5.0-alpha"), Some((0, 5, 0)));
    }

    #[test]
    fn parses_a_version_with_no_prerelease_suffix() {
        assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
    }

    #[test]
    fn rejects_an_unparseable_version() {
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver("v1.2"), None);
        assert_eq!(parse_semver(""), None);
    }

    #[test]
    fn detects_a_newer_minor_version() {
        assert!(is_newer("0.4.0-alpha", "0.5.0-alpha"));
    }

    #[test]
    fn detects_a_newer_patch_version() {
        assert!(is_newer("0.5.0-alpha", "0.5.1-alpha"));
    }

    #[test]
    fn does_not_flag_the_same_version_as_newer() {
        assert!(!is_newer("0.5.0-alpha", "0.5.0-alpha"));
    }

    #[test]
    fn does_not_flag_an_older_version_as_newer() {
        assert!(!is_newer("0.5.0-alpha", "0.4.0-alpha"));
    }

    #[test]
    fn ignores_prerelease_suffix_differences_and_compares_only_the_numeric_core() {
        // 0.5.0-beta isn't "newer" than 0.5.0-alpha by this comparison —
        // deliberately simple (numeric core only), not a full semver
        // prerelease-precedence implementation.
        assert!(!is_newer("0.5.0-alpha", "0.5.0-beta"));
    }

    #[test]
    fn fails_closed_when_either_version_is_unparseable() {
        assert!(!is_newer("garbage", "0.5.0-alpha"));
        assert!(!is_newer("0.5.0-alpha", "garbage"));
        assert!(!is_newer("garbage", "also-garbage"));
    }
}
