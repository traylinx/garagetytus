//! `StorageBackend` trait — LD#13 per
//! `verdicts/RUSTFS-COMPARISON-2026-04-25.md`.
//!
//! v0.1 ships exactly one impl: `GarageBackend` (lives in the
//! `garagetytus` CLI crate, where the lifecycle commands also
//! live). The trait is here at the workspace top so that a future
//! `RustfsBackend` (re-evaluation 2027-04-25) is a 1-day drop-in,
//! not a v2 rewrite.
//!
//! Hard rule: no `cfg!(feature="rustfs")` flags in v0.1. The trait
//! is in place; the second impl waits for rustfs to hit 1.0 stable
//! + named production users + first-class Mac/Windows binaries.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;

/// Per-call context. Trait impls receive a borrow of this on every
/// method so they can resolve paths without owning a `home`. The
/// `home` field is the garagetytus state root — see
/// `crate::paths::home_dir()`.
#[derive(Debug, Clone)]
pub struct Ctx<'a> {
    pub home: &'a Path,
    pub admin_url: &'a str,
    pub admin_token: &'a str,
}

/// The (intentionally narrow) bucket lifecycle surface. v0.1 ships
/// exactly the methods that `bucket.rs` already calls; new methods
/// land alongside new commands, never speculatively.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn create_bucket(&self, ctx: &Ctx<'_>, name: &str) -> Result<()>;
    async fn delete_bucket(&self, ctx: &Ctx<'_>, name: &str) -> Result<()>;
    async fn create_key(
        &self,
        ctx: &Ctx<'_>,
        label: &str,
    ) -> Result<(String, String)>;
    async fn delete_key(&self, ctx: &Ctx<'_>, key_id: &str) -> Result<()>;
    async fn allow(
        &self,
        ctx: &Ctx<'_>,
        bucket: &str,
        key_id: &str,
        perms: &str,
    ) -> Result<()>;
    async fn deny_all(
        &self,
        ctx: &Ctx<'_>,
        bucket: &str,
        denied: bool,
    ) -> Result<()>;
    async fn bucket_size(&self, ctx: &Ctx<'_>, bucket: &str) -> Result<u64>;

    /// Backend identifier (`"garage"`, `"rustfs"`, …). Consumers
    /// must NOT branch on impl type — only on this name when the
    /// distinction is unavoidable.
    fn name(&self) -> &'static str;

    /// Minimum upstream version this impl is tested against. v0.1
    /// `GarageBackend` returns `"v2.3.0"`.
    fn min_supported_version(&self) -> &'static str;
}
