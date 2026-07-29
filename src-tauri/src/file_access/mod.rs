//! File Access: folder-level user consent gate for local filesystem reads.
//! Design: `Multi-AI Agent Panel Document/01 Project Overview/AI Guardrails (必守規則).md`
//!
//! The only way a grant is created is `storage::grant_folder_access`,
//! which the frontend only calls after the user picks a folder through the
//! OS's native folder picker (`@tauri-apps/plugin-dialog`) — the picker
//! itself *is* the explicit-consent step per the Architecture decision
//! ("預設以資料夾為授權單位"). This module never creates a grant on its
//! own; it only checks and reads.

use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::Storage;

#[derive(Debug, PartialEq, Eq)]
pub enum FileAccessError {
    /// No grant covers this path at all. Error Code Registry: E5001.
    NotAuthorized,
    /// The path resolves outside every granted folder (e.g. `..` tricks,
    /// or a symlink escaping the grant). Error Code Registry: E5004.
    OutOfScope,
    /// The path is inside a granted folder but doesn't exist.
    /// Error Code Registry: E5002.
    NotFound,
    /// OS denied the read even though the grant covers it (permissions,
    /// file locked, etc). Error Code Registry: E5003.
    PermissionDenied(String),
}

impl FileAccessError {
    pub fn error_code(&self) -> &'static str {
        match self {
            FileAccessError::NotAuthorized => "E5001",
            FileAccessError::NotFound => "E5002",
            FileAccessError::PermissionDenied(_) => "E5003",
            FileAccessError::OutOfScope => "E5004",
        }
    }
}

impl std::fmt::Display for FileAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileAccessError::NotAuthorized => write!(f, "{} not authorized to read this path", self.error_code()),
            FileAccessError::OutOfScope => write!(f, "{} path resolves outside the granted folder", self.error_code()),
            FileAccessError::NotFound => write!(f, "{} file not found", self.error_code()),
            FileAccessError::PermissionDenied(msg) => write!(f, "{} {msg}", self.error_code()),
        }
    }
}

/// True if `path` (once resolved) lives inside any of `granted_folders`
/// (also resolved). Resolving both sides closes off `..`/symlink escapes —
/// a grant on `C:\notes` does not implicitly cover `C:\notes\..\secrets`.
fn is_within_granted_folders(granted_folders: &[String], path: &Path) -> Result<bool, FileAccessError> {
    let resolved = fs::canonicalize(path).map_err(|_| FileAccessError::NotFound)?;
    for folder in granted_folders {
        if let Ok(resolved_folder) = fs::canonicalize(folder) {
            if resolved.starts_with(&resolved_folder) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Reads a file on the agent's behalf, enforcing that it falls under one
/// of the agent's granted folders. This is the only read path — there is
/// no bypass that skips the authorization check.
pub fn read_file(storage: &Storage, agent_id: &str, path: &Path) -> Result<String, FileAccessError> {
    let grants = storage
        .list_file_access_grants(agent_id)
        .map_err(|_| FileAccessError::NotAuthorized)?;
    let folders: Vec<String> = grants.into_iter().map(|g| g.folder_path).collect();

    if folders.is_empty() {
        return Err(FileAccessError::NotAuthorized);
    }

    match is_within_granted_folders(&folders, path) {
        Ok(true) => {}
        Ok(false) => return Err(FileAccessError::OutOfScope),
        Err(FileAccessError::NotFound) => return Err(FileAccessError::NotFound),
        Err(other) => return Err(other),
    }

    fs::read_to_string(path).map_err(|e| FileAccessError::PermissionDenied(e.to_string()))
}

/// Lists the granted-folder paths as an absolute `PathBuf`, for display.
pub fn granted_folders(storage: &Storage, agent_id: &str) -> Vec<PathBuf> {
    storage
        .list_file_access_grants(agent_id)
        .unwrap_or_default()
        .into_iter()
        .map(|g| PathBuf::from(g.folder_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("map-file-access-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_a_file_inside_a_granted_folder() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("granted");
        storage.grant_folder_access(&agent.id, dir.to_str().unwrap()).unwrap();

        let file_path = dir.join("note.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        write!(file, "hello from the vault").unwrap();

        let content = read_file(&storage, &agent.id, &file_path).unwrap();
        assert_eq!(content, "hello from the vault");
    }

    #[test]
    fn rejects_reads_with_no_grant_at_all() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("no-grant");
        let file_path = dir.join("note.txt");
        fs::File::create(&file_path).unwrap();

        let err = read_file(&storage, &agent.id, &file_path).unwrap_err();
        assert_eq!(err, FileAccessError::NotAuthorized);
        assert_eq!(err.error_code(), "E5001");
    }

    #[test]
    fn rejects_reads_outside_the_granted_folder() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let granted_dir = temp_dir("scope-granted");
        let other_dir = temp_dir("scope-other");
        storage.grant_folder_access(&agent.id, granted_dir.to_str().unwrap()).unwrap();

        let file_path = other_dir.join("secret.txt");
        fs::File::create(&file_path).unwrap();

        let err = read_file(&storage, &agent.id, &file_path).unwrap_err();
        assert_eq!(err, FileAccessError::OutOfScope);
        assert_eq!(err.error_code(), "E5004");
    }

    #[test]
    fn reports_missing_file_inside_a_granted_folder() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("missing");
        storage.grant_folder_access(&agent.id, dir.to_str().unwrap()).unwrap();

        let err = read_file(&storage, &agent.id, &dir.join("does-not-exist.txt")).unwrap_err();
        assert_eq!(err, FileAccessError::NotFound);
        assert_eq!(err.error_code(), "E5002");
    }

    #[test]
    fn one_agents_grant_does_not_authorize_another_agent() {
        let storage = Storage::open_in_memory().unwrap();
        let granted_agent = storage.create_agent("Granted", None, None, "cloud", "anthropic", "claude").unwrap();
        let other_agent = storage.create_agent("Other", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("per-agent");
        storage.grant_folder_access(&granted_agent.id, dir.to_str().unwrap()).unwrap();

        let file_path = dir.join("note.txt");
        fs::File::create(&file_path).unwrap();

        assert!(read_file(&storage, &granted_agent.id, &file_path).is_ok());
        assert_eq!(
            read_file(&storage, &other_agent.id, &file_path).unwrap_err(),
            FileAccessError::NotAuthorized
        );
    }
}
