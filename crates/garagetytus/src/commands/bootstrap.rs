//! `garagetytus bootstrap` — Phase A.2 carve-out from the
//! `s3 bootstrap` body in `makakoo-os/makakoo/src/commands/s3.rs`.
//!
//! v0.1 stub: defers to a Phase B implementation that hits the
//! Garage admin API. Wires the daemon's first-run setup: assigns
//! the storage layout, writes the admin token to keychain, makes
//! the daemon ready to accept bucket commands.

use anyhow::Result;

use crate::context::CliContext;

pub async fn run(_ctx: &CliContext) -> Result<i32> {
    println!("garagetytus bootstrap: TODO Phase B — admin-API layout assignment.");
    Ok(0)
}
