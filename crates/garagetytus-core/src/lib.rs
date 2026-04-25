//! `garagetytus-core` — cross-platform path resolution, keychain
//! abstraction, and the `StorageBackend` trait (LD#13).
//!
//! This crate is platform-aware. The bucket lifecycle code in the
//! main `garagetytus` CLI crate dispatches through the
//! [`StorageBackend`] trait so that a future `RustfsBackend` impl
//! becomes a drop-in once rustfs hits 1.0 (re-evaluation 2027-04-25
//! per `verdicts/RUSTFS-COMPARISON-2026-04-25.md`).
//!
//! v0.1 ships exactly one impl: `GarageBackend`, which lives in
//! the `garagetytus` CLI crate (kept there because it shells out
//! to the `garage` binary on PATH and so co-locates naturally with
//! the lifecycle commands).

pub mod backend;
pub mod paths;
pub mod secrets;

pub use backend::{Ctx, StorageBackend};
pub use paths::{config_dir, data_dir, home_dir, log_dir, GARAGETYTUS_HOME_ENV};
pub use secrets::{SecretsStore, SERVICE};
