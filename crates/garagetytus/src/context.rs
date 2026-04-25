//! `CliContext` — minimal per-invocation state.
//!
//! Mirrors the `home() -> &Path` surface used by the carved
//! `commands::bucket` module, plus a few convenience accessors
//! for paths/URLs that several commands share. Everything else is
//! resolved on demand via `garagetytus_core::paths` so the context
//! stays lightweight.

// Some accessors on CliContext are read by future Phase B
// commands that haven't surfaced yet (admin_url / s3_url consumed
// by health probes & metrics endpoint in v0.2). Keep them exported
// without the dead_code warnings.
#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::Result;

/// Default Garage admin API URL — `http://127.0.0.1:3903`. Bound by
/// `bootstrap.rs` and consumed by every `bucket` subcommand.
pub const DEFAULT_ADMIN_URL: &str = "http://127.0.0.1:3903";
/// Default Garage S3 API URL — `http://127.0.0.1:3900`.
pub const DEFAULT_S3_URL: &str = "http://127.0.0.1:3900";

#[derive(Debug, Clone)]
pub struct CliContext {
    home: PathBuf,
}

impl CliContext {
    pub fn new() -> Result<Self> {
        let home = garagetytus_core::paths::home_dir();
        std::fs::create_dir_all(&home).ok();
        Ok(Self { home })
    }

    /// State root. `~/.garagetytus/` on a fresh install with
    /// `GARAGETYTUS_HOME` unset, otherwise `dirs::data_dir()`-derived
    /// per OS. See `garagetytus_core::paths` for the layout matrix.
    pub fn home(&self) -> &PathBuf {
        &self.home
    }

    /// Garage admin API URL.
    pub fn admin_url(&self) -> &'static str {
        DEFAULT_ADMIN_URL
    }

    /// Garage S3 API URL.
    pub fn s3_url(&self) -> &'static str {
        DEFAULT_S3_URL
    }
}
