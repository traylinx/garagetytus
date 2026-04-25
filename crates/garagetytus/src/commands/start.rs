//! `garagetytus start / stop / status / restart / serve` — daemon
//! lifecycle (Phase B.2). v0.1 wires the macOS path via launchctl;
//! Linux path via systemctl --user; Windows is the v0.2 deferral.

use anyhow::Result;

use crate::context::CliContext;

const WINDOWS_DEFERRAL: &str =
    "v0.1 ships Mac + Linux only. Windows support targets v0.2.";

pub fn run(ctx: &CliContext, restart: bool) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }
    let _ = ctx;
    if restart {
        println!("garagetytus restart: TODO Phase B.2 — calls stop + start atomically.");
    } else {
        println!("garagetytus start: TODO Phase B.2 — `launchctl load` / `systemctl --user start`.");
    }
    Ok(0)
}

pub fn stop(_ctx: &CliContext) -> Result<i32> {
    println!("garagetytus stop: TODO Phase B.2.");
    Ok(0)
}

pub fn status(_ctx: &CliContext) -> Result<i32> {
    println!("garagetytus status: TODO Phase B.2.");
    Ok(0)
}

pub fn serve(_ctx: &CliContext) -> Result<i32> {
    println!("garagetytus serve: TODO Phase B.2 — runs `garage server -c <config>` in foreground.");
    Ok(0)
}
