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

/// Caps for `list_text_files_in_grants` — this walks a whole granted
/// folder rather than reading one file the agent asked for by name, so
/// unlike `read_file` it needs its own defensive bounds: a folder
/// someone granted access to could contain thousands of files, or (as
/// happened once in this project's own vault — a `.docx` mistakenly
/// saved with a `.md` extension) a multi-megabyte binary blob wearing a
/// text extension.
const MAX_INDEXABLE_FILES: usize = 200;
const MAX_INDEXABLE_FILE_BYTES: u64 = 200 * 1024;
const INDEXABLE_EXTENSIONS: &[&str] = &["md", "txt"];

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
/// of the agent's *effective* granted folders — its own private grants,
/// plus anything shared to a Group Chat session it's currently in (see
/// `storage::effective_granted_folders`). This is the only read path —
/// there is no bypass that skips the authorization check.
pub fn read_file(storage: &Storage, agent_id: &str, path: &Path) -> Result<String, FileAccessError> {
    let folders = storage
        .effective_granted_folders(agent_id)
        .map_err(|_| FileAccessError::NotAuthorized)?;

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

/// Collects `(path, content)` pairs for every `.md`/`.txt` file inside
/// `agent_id`'s granted folders (recursively), for feeding into the
/// `ml_engine`'s `semantic_search` capability (see `ML Engine Design.md`
/// in the vault). Every file returned lives under a path the user
/// explicitly granted — this walks `grant.folder_path` itself rather
/// than accepting a path from the caller, so there's no `..` escape to
/// defend against the way `read_file` has to. Unreadable, oversized, or
/// non-UTF8 files are silently skipped rather than failing the whole
/// scan, same policy as `skill_manager::discover_skills`.
pub fn list_text_files_in_grants(storage: &Storage, agent_id: &str) -> Vec<(String, String)> {
    let folders = storage.effective_granted_folders(agent_id).unwrap_or_default();
    let mut results = Vec::new();
    for folder in folders {
        if results.len() >= MAX_INDEXABLE_FILES {
            break;
        }
        collect_text_files(&PathBuf::from(&folder), &mut results);
    }
    results.truncate(MAX_INDEXABLE_FILES);
    results
}

fn collect_text_files(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_INDEXABLE_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_text_files(&path, out);
            continue;
        }
        let is_indexable_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| INDEXABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_indexable_extension {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_INDEXABLE_FILE_BYTES {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            out.push((path.to_string_lossy().to_string(), text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

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

    #[test]
    fn a_group_chat_shared_grant_lets_a_fellow_member_read_the_file() {
        let storage = Storage::open_in_memory().unwrap();
        let picker = storage.create_agent("Picker", None, None, "cloud", "anthropic", "claude").unwrap();
        let teammate = storage.create_agent("Teammate", None, None, "cloud", "anthropic", "claude").unwrap();
        let outsider = storage.create_agent("Outsider", None, None, "cloud", "anthropic", "claude").unwrap();
        let session = storage.create_session("group", "Standup").unwrap();
        storage.add_agent_to_session(&session.id, &picker.id).unwrap();
        storage.add_agent_to_session(&session.id, &teammate.id).unwrap();

        let dir = temp_dir("group-shared");
        storage.grant_folder_access_for_session(&session.id, &picker.id, dir.to_str().unwrap()).unwrap();

        let file_path = dir.join("note.txt");
        fs::write(&file_path, "shared with the whole meeting").unwrap();

        assert_eq!(read_file(&storage, &picker.id, &file_path).unwrap(), "shared with the whole meeting");
        assert_eq!(read_file(&storage, &teammate.id, &file_path).unwrap(), "shared with the whole meeting");
        assert_eq!(read_file(&storage, &outsider.id, &file_path).unwrap_err(), FileAccessError::NotAuthorized);
    }

    #[test]
    fn list_text_files_in_grants_only_returns_indexable_extensions() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("indexable-ext");
        storage.grant_folder_access(&agent.id, dir.to_str().unwrap()).unwrap();

        fs::write(dir.join("notes.md"), "# hello").unwrap();
        fs::write(dir.join("readme.txt"), "plain text").unwrap();
        fs::write(dir.join("data.bin"), [0u8, 159, 146, 150]).unwrap();

        let mut files = list_text_files_in_grants(&storage, &agent.id);
        files.sort();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|(path, _)| path.ends_with("notes.md")));
        assert!(files.iter().any(|(path, _)| path.ends_with("readme.txt")));
    }

    #[test]
    fn list_text_files_in_grants_walks_subdirectories() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("nested");
        storage.grant_folder_access(&agent.id, dir.to_str().unwrap()).unwrap();

        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/deep.md"), "deep note").unwrap();

        let files = list_text_files_in_grants(&storage, &agent.id);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, "deep note");
    }

    #[test]
    fn list_text_files_in_grants_skips_oversized_files() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        let dir = temp_dir("oversized");
        storage.grant_folder_access(&agent.id, dir.to_str().unwrap()).unwrap();

        fs::write(dir.join("huge.md"), "x".repeat(300 * 1024)).unwrap();
        fs::write(dir.join("normal.md"), "fine").unwrap();

        let files = list_text_files_in_grants(&storage, &agent.id);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, "fine");
    }

    #[test]
    fn list_text_files_in_grants_is_empty_with_no_grants() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        assert!(list_text_files_in_grants(&storage, &agent.id).is_empty());
    }
}
