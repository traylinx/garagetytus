//! `garagetytus install` — cross-platform installer (Phase B.1).
//!
//! Three OS branches (LD#7 amended 2026-04-25 per Q1 verdict):
//!
//! - **Linux:** download upstream `*-musl/garage` binary from
//!   `garagehq.deuxfleurs.fr/_releases/v2.3.0/...`, SHA-pin verify,
//!   unpack to `~/.local/bin/garage`. Generate config via the
//!   hand-rolled launchd/systemd templates (LD#3 fallback).
//! - **macOS:** three-branch detection — Homebrew missing → "install
//!   Homebrew first"; brew present + garage missing → "run `brew
//!   install garage`"; both present → generate config + plist +
//!   `launchctl load`.
//! - **Windows:** print v0.2-deferral message, exit 0.
//!
//! v0.1 ships the deferral notice + the macOS branch (matches the
//! existing v0.7.1 plugins-core/garage-store/bin/install.sh path).
//! Linux branch lands when the install workflow is wired in Phase B.

use anyhow::Result;

use crate::context::CliContext;

const WINDOWS_DEFERRAL: &str =
    "v0.1 ships Mac + Linux only. Windows support targets v0.2. \
See README §Windows.";

pub async fn run(_ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    #[cfg(target_os = "macos")]
    {
        return mac_install(_ctx).await;
    }

    #[cfg(target_os = "linux")]
    {
        return linux_install(_ctx).await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!(
            "unsupported target — garagetytus v0.1 supports macOS, Linux, and Windows-deferred (got {})",
            std::env::consts::OS
        );
    }
}

#[cfg(target_os = "macos")]
async fn mac_install(_ctx: &CliContext) -> Result<i32> {
    use std::process::Command;
    let brew = Command::new("which").arg("brew").output()?;
    if !brew.status.success() {
        eprintln!("garagetytus install: Homebrew not found.");
        eprintln!("  Install Homebrew first: https://brew.sh");
        return Ok(1);
    }
    let garage = Command::new("which").arg("garage").output()?;
    if !garage.status.success() {
        eprintln!("garagetytus install: `garage` not on PATH.");
        eprintln!("  Run: brew install garage");
        eprintln!("  Or:  brew install traylinx/tap/garagetytus (also pulls Garage)");
        return Ok(1);
    }
    println!("garagetytus install: Homebrew + garage detected on macOS.");
    println!("  TODO Phase B.1: seed config + plist via service-manager fallback templates.");
    Ok(0)
}

#[cfg(target_os = "linux")]
async fn linux_install(_ctx: &CliContext) -> Result<i32> {
    println!("garagetytus install: Linux flow.");
    println!("  TODO Phase B.1: download upstream musl binary, SHA-verify, write systemd-user unit.");
    Ok(0)
}
