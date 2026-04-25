//! `garagetytus-grants` — runtime user-grants store + audit log +
//! rate-limit gate, carved from Makakoo v0.7.1 capability subsystem
//! (2026-04-25 per GARAGETYTUS-V0.1 Phase A.5).
//!
//! This crate owns three tightly-coupled primitives that all live
//! beside one another in the bucket grant flow:
//!
//! * [`user_grants`] — `~/.garagetytus/grants.json` writer/reader.
//!   garagetytus is the **sole writer** (LD#9 — Makakoo + tytus
//!   read this file via the same on-disk format).
//! * [`rate_limit`] — global active-grant + create-rate guard.
//!   Counter lives in a separate file so a corrupt counter cannot
//!   poison the grants (Lope F7).
//! * [`audit`] / [`audit_escape`] — append-only audit log + the
//!   field-escape helper that keeps log lines parseable when grant
//!   labels contain control characters.
//!
//! All three modules are pure carve-outs (no semantic changes) from
//! `makakoo-os/makakoo-core/src/capability/{user_grants,rate_limit,
//! audit,audit_escape}.rs`. Schema version 1 is frozen across both
//! repos. Mutating the schema requires coordinated updates here AND
//! in `garagetytus-sdk` (Phase C — Python mirror).

pub mod audit;
pub mod audit_escape;
pub mod rate_limit;
pub mod user_grants;

// Backward-compat flat re-exports — match the
// `makakoo_core::capability::*` surface so consumers that imported
// at the flat level (bucket.rs, perms.rs, etc.) keep working.

pub use audit::{AuditEntry, AuditLog, AuditResult, RotationError};
pub use audit_escape::escape_audit_field;
pub use rate_limit::{
    check_and_increment as rate_limit_check_and_increment,
    decrement as rate_limit_decrement, RateLimitError, MAX_ACTIVE_GRANTS,
    MAX_CREATES_PER_HOUR,
};
pub use user_grants::{
    default_path, glob_match, new_grant_id, UserGrant, UserGrants,
    UserGrantsError, SCHEMA_VERSION,
};
