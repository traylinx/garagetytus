//! Cross-platform credential storage via the `keyring` crate.
//!
//! Carved from `makakoo-os/makakoo/src/secrets.rs` (137 LOC, MIT)
//! 2026-04-25 per Phase A.1. Only change: service namespace is now
//! `garagetytus` instead of `makakoo`.
//!
//! Per LD#5, no direct calls to `security` / `secret-tool` /
//! `cmdkey` — everything goes through this façade. The Python SDK
//! mirror in Phase C uses the `keyring` pip package and points at
//! the same service name.
//!
//! Per LD#6, headless-Linux fallback is opt-in via the
//! `--allow-file-creds` flag at the CLI surface; this façade does
//! NOT silently fall back.

#![allow(dead_code)]

use anyhow::Result;
use keyring::Entry;

/// Service namespace for every keyring entry written by garagetytus.
pub const SERVICE: &str = "garagetytus";

/// Thin façade around the `keyring` crate — all methods are static
/// and the struct exists purely as a namespace.
pub struct SecretsStore;

impl SecretsStore {
    /// Store a secret under `key`. Overwrites any prior value.
    pub fn set(key: &str, value: &str) -> Result<()> {
        let entry = Entry::new(SERVICE, key)?;
        entry.set_password(value)?;
        Ok(())
    }

    /// Retrieve a secret. Errors if the entry does not exist.
    pub fn get(key: &str) -> Result<String> {
        let entry = Entry::new(SERVICE, key)?;
        Ok(entry.get_password()?)
    }

    /// Delete a secret. Idempotent — `Ok(())` if the entry was
    /// removed or was missing already.
    pub fn delete(key: &str) -> Result<()> {
        let entry = Entry::new(SERVICE, key)?;
        match entry.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Store a JSON-serialisable value under `key`.
    pub fn set_json<T: serde::Serialize>(key: &str, value: &T) -> Result<()> {
        let body = serde_json::to_string(value)?;
        Self::set(key, &body)
    }

    /// Retrieve a JSON-deserialisable value under `key`.
    pub fn get_json<T: serde::de::DeserializeOwned>(key: &str) -> Result<T> {
        let raw = Self::get(key)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Resolve a secret with an env-var fallback. Returns:
    ///   1. keyring value for `key` if present, else
    ///   2. value of `env_name` if set, else
    ///   3. None.
    pub fn resolve(key: &str, env_name: &str) -> Option<String> {
        if let Ok(v) = Self::get(key) {
            return Some(v);
        }
        std::env::var(env_name).ok()
    }
}

/// Canonical secret keys used across garagetytus.
pub mod keys {
    /// Per-grant credentials JSON `{access_key, secret_key,
    /// endpoint}`. Account name is `bucket-grant:<grant_id>`.
    pub const BUCKET_GRANT_PREFIX: &str = "bucket-grant:";
    /// Garage admin token, written by `garagetytus install` /
    /// `bootstrap`, read by `bucket` admin-API calls.
    pub const GARAGE_ADMIN_TOKEN: &str = "garage-admin-token";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_env_when_keyring_missing() {
        let env_name = "GARAGETYTUS_SECRET_TEST_ENV_VAR";
        let _ = SecretsStore::delete("NONEXISTENT_KEY_FOR_TEST");
        std::env::set_var(env_name, "from-env");
        let v = SecretsStore::resolve("NONEXISTENT_KEY_FOR_TEST", env_name);
        std::env::remove_var(env_name);
        assert_eq!(v, Some("from-env".to_string()));
    }

    #[test]
    fn resolve_returns_none_when_both_missing() {
        let env_name = "GARAGETYTUS_DEFINITELY_NOT_SET_9f8a7b";
        std::env::remove_var(env_name);
        let v = SecretsStore::resolve("ALSO_NOT_IN_KEYRING_9f8a7b", env_name);
        assert_eq!(v, None);
    }

    #[test]
    fn delete_is_idempotent() {
        SecretsStore::delete("GARAGETYTUS_DELETE_IDEMPOTENT_TEST").unwrap();
        SecretsStore::delete("GARAGETYTUS_DELETE_IDEMPOTENT_TEST").unwrap();
    }
}
