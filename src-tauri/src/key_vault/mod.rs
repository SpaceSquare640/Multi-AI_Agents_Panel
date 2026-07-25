//! Key Vault: local, encrypted storage for user-supplied cloud API keys.
//! No key ever leaves the user's machine.
//! Design: `Multi-AI Agent Panel Document/01 Project Overview/Tech Stack.md`
//!
//! Backed by the OS-native credential store (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service) via the `keyring` crate, per the
//! decision in Tech Stack.md to prefer native keychains over a bespoke
//! encryption scheme.

use keyring::Entry;

const SERVICE: &str = "multi-ai-agents-panel";

fn entry(provider: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, provider)
}

/// Store (or overwrite) the API key for a provider, e.g. "openai", "anthropic", "openrouter".
pub fn set_api_key(provider: &str, key: &str) -> keyring::Result<()> {
    entry(provider)?.set_password(key)
}

/// Look up the API key for a provider. Returns `Ok(None)` if nothing is stored yet.
pub fn get_api_key(provider: &str) -> keyring::Result<Option<String>> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Remove the stored API key for a provider, if any.
pub fn delete_api_key(provider: &str) -> keyring::Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These hit the real OS credential store, so they use a provider name
    // that won't collide with anything real, and clean up after themselves.
    const TEST_PROVIDER: &str = "map-key-vault-test";

    #[test]
    fn set_get_delete_round_trip() {
        delete_api_key(TEST_PROVIDER).ok();

        assert_eq!(get_api_key(TEST_PROVIDER).unwrap(), None);

        set_api_key(TEST_PROVIDER, "sk-test-123").unwrap();
        assert_eq!(
            get_api_key(TEST_PROVIDER).unwrap(),
            Some("sk-test-123".to_string())
        );

        delete_api_key(TEST_PROVIDER).unwrap();
        assert_eq!(get_api_key(TEST_PROVIDER).unwrap(), None);
    }
}
