//! LD#1 hard-fail gate (Phase B AGPL contract).
//!
//! Checks the lockfile + every Cargo.toml in the workspace for any
//! `garage-*` crate dependency. If found, fails the build — the
//! subprocess-only contract demands zero linking against Garage.
//!
//! This is a coarse grep test, not a substitute for `cargo-deny`,
//! which the v0.1 release pipeline (Phase B.4) wires in alongside.
//! Both gates fire on every PR.

use std::fs;
use std::path::PathBuf;

const FORBIDDEN_PREFIXES: &[&str] = &["garage_", "garage-"];

#[test]
fn lockfile_contains_no_garage_crates() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../");
    let lockfile = repo_root.join("Cargo.lock");
    if !lockfile.exists() {
        eprintln!(
            "Cargo.lock missing at {} — skipping (CI must fail this if reached)",
            lockfile.display()
        );
        return;
    }
    let content = fs::read_to_string(&lockfile).expect("read Cargo.lock");
    let mut hits: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("name = ") {
            // Form: name = "<crate>"
            let name_quoted = trimmed.trim_start_matches("name = ").trim();
            let name = name_quoted.trim_matches('"');
            for prefix in FORBIDDEN_PREFIXES {
                if name.starts_with(prefix) {
                    hits.push(name.to_string());
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "LD#1 violation — Cargo.lock contains forbidden garage-* crates: {:?}. \
         garagetytus orchestrates Garage as a child process and must NEVER link \
         against any garage-* Rust crate. See THIRD_PARTY_NOTICES.",
        hits
    );
}
