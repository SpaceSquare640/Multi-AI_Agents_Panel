//! Key Vault: local, encrypted storage for user-supplied cloud API keys.
//! No key ever leaves the user's machine.
//! Design: `Multi-AI Agent Panel Document/01 Project Overview/Tech Stack.md`
//!
//! Backed by the OS-native credential store (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service) via the `keyring` crate, per the
//! decision in Tech Stack.md to prefer native keychains over a bespoke
//! encryption scheme.
//!
//! Secrets are addressed by an opaque entry id (a UUID minted by
//! `storage::create_provider_key`), not by provider name — a user can hold
//! several keys for the same provider (e.g. several free OpenRouter keys,
//! one per model), so "provider" is metadata that lives in `storage`, not
//! the credential store's lookup key.

use keyring::Entry;

const SERVICE: &str = "multi-ai-agents-panel";

fn entry(id: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, id)
}

/// Store (or overwrite) the secret for an entry id.
pub fn set_secret(id: &str, value: &str) -> keyring::Result<()> {
    entry(id)?.set_password(value)
}

/// Look up the secret for an entry id. Returns `Ok(None)` if nothing is stored.
pub fn get_secret(id: &str) -> keyring::Result<Option<String>> {
    match entry(id)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Remove the stored secret for an entry id, if any.
pub fn delete_secret(id: &str) -> keyring::Result<()> {
    match entry(id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These hit the real OS credential store, so they use an id that won't
    // collide with anything real, and clean up after themselves.
    const TEST_ID: &str = "map-key-vault-test";

    #[test]
    fn set_get_delete_round_trip() {
        delete_secret(TEST_ID).ok();

        assert_eq!(get_secret(TEST_ID).unwrap(), None);

        set_secret(TEST_ID, "sk-test-123").unwrap();
        assert_eq!(get_secret(TEST_ID).unwrap(), Some("sk-test-123".to_string()));

        delete_secret(TEST_ID).unwrap();
        assert_eq!(get_secret(TEST_ID).unwrap(), None);
    }
}
