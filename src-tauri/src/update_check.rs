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
//! explicitly excludes prereleases, and this project deliberately offers
//! prereleases to everyone: a beta exists to be tried, and a beta nobody
//! is told about is a beta nobody tests. The result carries
//! `is_prerelease` so the UI can say plainly which kind it is offering
//! rather than presenting a beta as though it were a stable release.

use serde::{Deserialize, Serialize};

const RELEASES_URL: &str = "https://api.github.com/repos/SpaceSquare640/Multi-AI_Agents_Panel/releases?per_page=1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    /// Whether the release being offered is marked as a prerelease on
    /// GitHub. Surfaced rather than filtered: the check offers betas by
    /// design, so the honest thing is to label them, not to hide what
    /// kind of build the user is being pointed at.
    pub is_prerelease: bool,
}

/// A parsed version: the numeric core, plus the prerelease identifiers
/// after the first `-` (empty for a stable release).
///
/// The prerelease part used to be discarded outright, which was harmless
/// only while *every* release was a prerelease. The moment stable builds
/// and betas coexist it stops being harmless: `1.7.0-beta` and `1.7.0`
/// reduce to the same three numbers, so someone running the beta is told
/// they are up to date forever and never learns the finished release
/// shipped. Keeping the identifiers is what fixes that.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    core: (u64, u64, u64),
    /// Dot-separated identifiers from the prerelease part. Empty means a
    /// stable release, which by semver outranks any prerelease of the
    /// same core version.
    pre: Vec<String>,
}

/// Parses `v1.2.3-beta.1` and friends. Returns `None` for anything that
/// doesn't fit `major.minor.patch[-pre]` rather than guessing — an
/// unparseable version should never be silently treated as "up to date"
/// or "update available."
fn parse_semver(version: &str) -> Option<Version> {
    let without_v = version.trim_start_matches('v');
    // Build metadata (`+sha`) carries no precedence in semver; drop it.
    let without_build = without_v.split('+').next()?;
    // A dash with nothing after it (`1.2.3-`) is malformed, and is not
    // the same thing as having no dash at all — distinguished here
    // rather than by the empty-identifier check below, which cannot tell
    // the two apart once both have collapsed to an empty string.
    let (core_str, pre_str) = match without_build.split_once('-') {
        Some((_, "")) => return None,
        Some((core, pre)) => (core, pre),
        None => (without_build, ""),
    };

    let mut parts = core_str.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let pre = if pre_str.is_empty() {
        Vec::new()
    } else {
        // An empty identifier (`1.0.0-`, `1.0.0-a..b`) is malformed.
        if pre_str.split('.').any(str::is_empty) {
            return None;
        }
        pre_str.split('.').map(str::to_string).collect()
    };

    Some(Version { core: (major, minor, patch), pre })
}

/// Semver prerelease precedence (spec §11), which is not plain string
/// ordering and is easy to get subtly wrong:
///
/// - a stable release outranks any prerelease of the same core version
///   (`1.7.0` > `1.7.0-beta`), which is the case this whole change exists
///   for;
/// - numeric identifiers compare numerically, so `beta.10` > `beta.9` —
///   lexically it would be the other way round;
/// - a numeric identifier ranks below a non-numeric one;
/// - if all shared identifiers tie, more identifiers wins
///   (`beta.1` > `beta`).
fn compare_prerelease(a: &[String], b: &[String]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        // Empty means "stable", which is greater.
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (x, y) in a.iter().zip(b.iter()) {
        let ordering = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xn), Ok(yn)) => xn.cmp(&yn),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

/// Pure comparison, split out from the network call so it's fully
/// unit-testable: `true` only if `latest` is strictly newer than
/// `current`. Unparseable input on either side fails closed (`false`,
/// "no update"), not open — a version-check bug should never nag a user
/// who's already current, or worse, mislead them into thinking a broken
/// comparison means they're current when it couldn't actually tell.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(c), Some(l)) => match l.core.cmp(&c.core) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => compare_prerelease(&l.pre, &c.pre) == std::cmp::Ordering::Greater,
        },
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
}

/// Fetches the most recently published release (including prereleases)
/// and compares it to `current_version`. GitHub's REST API requires a
/// `User-Agent` header on every request — no API key needed for public
/// unauthenticated reads like this one, but requests without a
/// `User-Agent` are rejected outright.
pub fn check_for_update(current_version: &str) -> Result<UpdateCheckResult, String> {
    let client = crate::http::metadata();
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
        is_prerelease: latest.prerelease,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(core: (u64, u64, u64), pre: &[&str]) -> Version {
        Version { core, pre: pre.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn parses_a_plain_alpha_version() {
        assert_eq!(parse_semver("0.5.0-alpha"), Some(version((0, 5, 0), &["alpha"])));
    }

    #[test]
    fn parses_a_v_prefixed_tag() {
        assert_eq!(parse_semver("v0.5.0-alpha"), Some(version((0, 5, 0), &["alpha"])));
    }

    #[test]
    fn parses_a_version_with_no_prerelease_suffix() {
        assert_eq!(parse_semver("1.0.0"), Some(version((1, 0, 0), &[])));
    }

    #[test]
    fn parses_dotted_prerelease_identifiers() {
        assert_eq!(parse_semver("v1.7.0-beta.2"), Some(version((1, 7, 0), &["beta", "2"])));
    }

    #[test]
    fn drops_build_metadata_which_carries_no_precedence() {
        assert_eq!(parse_semver("1.7.0-beta+abc123"), Some(version((1, 7, 0), &["beta"])));
        assert_eq!(parse_semver("1.7.0+abc123"), Some(version((1, 7, 0), &[])));
    }

    #[test]
    fn rejects_an_unparseable_version() {
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver("v1.2"), None);
        assert_eq!(parse_semver(""), None);
        // Four numeric components is not semver.
        assert_eq!(parse_semver("1.2.3.4"), None);
        // Empty prerelease identifiers are malformed.
        assert_eq!(parse_semver("1.2.3-"), None);
        assert_eq!(parse_semver("1.2.3-a..b"), None);
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
    fn a_stable_release_is_newer_than_the_prerelease_of_the_same_version() {
        // The bug this replaced: both sides reduced to (1, 7, 0), so
        // someone running the beta was told they were current forever and
        // never learned the finished release had shipped.
        assert!(is_newer("1.7.0-beta", "1.7.0"));
        assert!(!is_newer("1.7.0", "1.7.0-beta"));
    }

    #[test]
    fn prerelease_identifiers_order_by_semver_precedence_not_string_order() {
        assert!(is_newer("0.5.0-alpha", "0.5.0-beta"));
        // Numeric identifiers compare as numbers: lexically "10" < "9".
        assert!(is_newer("1.7.0-beta.9", "1.7.0-beta.10"));
        assert!(!is_newer("1.7.0-beta.10", "1.7.0-beta.9"));
        // More identifiers wins when the shared ones tie.
        assert!(is_newer("1.7.0-beta", "1.7.0-beta.1"));
        // A numeric identifier ranks below a non-numeric one.
        assert!(is_newer("1.7.0-1", "1.7.0-alpha"));
    }

    #[test]
    fn a_newer_core_version_wins_regardless_of_prerelease_status() {
        assert!(is_newer("1.7.0", "1.8.0-beta"));
        assert!(!is_newer("1.8.0-beta", "1.7.0"));
    }

    #[test]
    fn fails_closed_when_either_version_is_unparseable() {
        assert!(!is_newer("garbage", "0.5.0-alpha"));
        assert!(!is_newer("0.5.0-alpha", "garbage"));
        assert!(!is_newer("garbage", "also-garbage"));
    }
}
