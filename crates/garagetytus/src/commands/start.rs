//! `garagetytus start / stop / status / restart / serve` — Phase B.2
//! daemon lifecycle.
//!
//! Per-OS branches:
//!
//! - **macOS**: `launchctl bootstrap` / `bootout` / `print` against
//!   the per-user GUI domain (`gui/<uid>`). The plist seeded by
//!   `garagetytus install` lives at
//!   `~/Library/LaunchAgents/com.traylinx.garagetytus.plist`.
//! - **Linux**: `systemctl --user {start,stop,restart,status}
//!   garagetytus.service`. Unit seeded by `garagetytus install` at
//!   `~/.config/systemd/user/garagetytus.service`.
//! - **Windows**: prints v0.2 deferral notice and exits 0.
//!
//! `garagetytus serve` runs the daemon in foreground via
//! `garage -c <config> server` for users who supply their own
//! supervisor (Docker, k8s, runit, manual launchctl).

// Per-OS cfg gating creates dead-code warnings on the non-active
// branches; suppress the noise for the whole module.
#![allow(dead_code)]

use std::process::Command;

use anyhow::Result;

use crate::context::CliContext;

const WINDOWS_DEFERRAL: &str =
    "v0.1 ships Mac + Linux only. Windows support targets v0.2.";
#[cfg(target_os = "macos")]
const PLIST_LABEL: &str = "com.traylinx.garagetytus";
#[cfg(target_os = "linux")]
const SERVICE_UNIT: &str = "garagetytus.service";

pub fn run(ctx: &CliContext, restart: bool) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        let _ = (ctx, restart);
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }
    if restart {
        let _ = stop(ctx);
        return start(ctx);
    }
    start(ctx)
}

#[allow(dead_code)]
fn start(_ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "macos")]
    {
        let plist = plist_path();
        if !plist.exists() {
            eprintln!(
                "garagetytus start: plist missing at {} — run `garagetytus install` first.",
                plist.display()
            );
            return Ok(1);
        }
        let uid = unsafe { libc_getuid() };
        let domain = format!("gui/{}", uid);
        let status = Command::new("launchctl")
            .args(["bootstrap", &domain, plist.to_str().unwrap_or("")])
            .status()?;
        if status.success() {
            println!("garagetytus start: loaded {} into {}", PLIST_LABEL, domain);
            Ok(0)
        } else if status.code() == Some(17) {
            // launchctl exit 17 = already loaded.
            println!("garagetytus start: already running.");
            Ok(0)
        } else {
            eprintln!(
                "garagetytus start: launchctl bootstrap failed (exit {})",
                status.code().unwrap_or(-1)
            );
            Ok(1)
        }
    }

    #[cfg(target_os = "linux")]
    {
        let unit_path = systemd_unit_path();
        if !unit_path.exists() {
            eprintln!(
                "garagetytus start: systemd unit missing at {} — run `garagetytus install` first.",
                unit_path.display()
            );
            return Ok(1);
        }
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()?;
        let status = Command::new("systemctl")
            .args(["--user", "start", SERVICE_UNIT])
            .status()?;
        if status.success() {
            println!("garagetytus start: started {}", SERVICE_UNIT);
            Ok(0)
        } else {
            Ok(status.code().unwrap_or(1))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        eprintln!("garagetytus start: unsupported OS");
        Ok(1)
    }
}

pub fn stop(_ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    #[cfg(target_os = "macos")]
    {
        let plist = plist_path();
        if !plist.exists() {
            println!("garagetytus stop: plist absent — already stopped.");
            return Ok(0);
        }
        let uid = unsafe { libc_getuid() };
        let domain = format!("gui/{}", uid);
        let target = format!("{}/{}", domain, PLIST_LABEL);
        let status = Command::new("launchctl")
            .args(["bootout", &target])
            .status()?;
        match status.code() {
            // 0  = success.
            // 3  = "No such process" — service entry exists but is
            //      not loaded (idempotent path during uninstall).
            // 36 = "Could not find specified service" — already
            //      booted out.
            Some(0) | Some(3) | Some(36) => {
                println!("garagetytus stop: stopped");
                Ok(0)
            }
            other => {
                eprintln!(
                    "garagetytus stop: launchctl bootout failed (exit {})",
                    other.unwrap_or(-1)
                );
                Ok(1)
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let status = Command::new("systemctl")
            .args(["--user", "stop", SERVICE_UNIT])
            .status()?;
        if status.success() {
            println!("garagetytus stop: stopped {}", SERVICE_UNIT);
            Ok(0)
        } else {
            Ok(status.code().unwrap_or(1))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Ok(1)
    }
}

pub fn status(_ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc_getuid() };
        let domain = format!("gui/{}", uid);
        let target = format!("{}/{}", domain, PLIST_LABEL);
        let out = Command::new("launchctl")
            .args(["print", &target])
            .output()?;
        if out.status.success() {
            // launchctl print emits ~50 LOC of state — show a summary.
            let body = String::from_utf8_lossy(&out.stdout);
            let pid = body
                .lines()
                .find(|l| l.trim().starts_with("pid ="))
                .map(|l| l.trim().trim_start_matches("pid = ").to_string())
                .unwrap_or_else(|| "?".into());
            let state = body
                .lines()
                .find(|l| l.trim().starts_with("state ="))
                .map(|l| l.trim().trim_start_matches("state = ").to_string())
                .unwrap_or_else(|| "?".into());
            println!(
                "garagetytus status: running (pid={}, state={})",
                pid, state
            );
            Ok(0)
        } else {
            println!("garagetytus status: stopped");
            Ok(0)
        }
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("systemctl")
            .args(["--user", "is-active", SERVICE_UNIT])
            .output()?;
        let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
        println!("garagetytus status: {}", state);
        Ok(0)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Ok(1)
    }
}

pub fn serve(_ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    let cfg = garagetytus_core::paths::config_dir().join("garagetytus.toml");
    if !cfg.exists() {
        eprintln!(
            "garagetytus serve: config missing at {} — run `garagetytus install` first.",
            cfg.display()
        );
        return Ok(1);
    }
    let garage = which_garage();
    println!(
        "garagetytus serve: foreground daemon — `{} -c {} server` (Ctrl-C to stop)",
        garage.display(),
        cfg.display()
    );

    // Spawn (1) the watchdog tick loop and (2) the LD#11 /metrics
    // HTTP server alongside garage. Cooperatively shut down via the
    // AtomicBool flag when garage exits.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let shutdown = Arc::new(AtomicBool::new(false));
    let watchdog_shutdown = shutdown.clone();
    let state_dir = garagetytus_core::paths::data_dir();
    let watchdog_state_dir = state_dir.clone();
    let watchdog_handle = std::thread::spawn(move || {
        spawn_watchdog_loop(watchdog_state_dir, watchdog_shutdown);
    });

    // Metrics server on its own tokio runtime — independent thread
    // so we don't have to assume `serve` already has a runtime in
    // scope (foreground invocations from `serve` come from the
    // `#[tokio::main]` runtime in main.rs anyway, but spawning on
    // a fresh runtime keeps the lifecycle obvious).
    let metrics_state_dir = state_dir.clone();
    let metrics_shutdown = shutdown.clone();
    let metrics_handle = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!("metrics server tokio runtime: {}", e);
                return;
            }
        };
        rt.block_on(async move {
            let server = super::metrics::serve_metrics(metrics_state_dir);
            tokio::select! {
                res = server => {
                    if let Err(e) = res {
                        tracing::warn!("metrics server stopped: {}", e);
                    }
                }
                _ = poll_shutdown(metrics_shutdown) => {
                    tracing::info!("metrics server: shutdown requested");
                }
            }
        });
    });

    let status = Command::new(&garage)
        .args(["-c", cfg.to_str().unwrap_or(""), "server"])
        .status()?;

    shutdown.store(true, Ordering::Relaxed);
    let _ = watchdog_handle.join();
    let _ = metrics_handle.join();

    Ok(status.code().unwrap_or(1))
}

/// Poll the shutdown flag every second; resolve when it's set.
/// Used to race against the metrics server in `tokio::select!`.
async fn poll_shutdown(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    loop {
        if flag.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Watchdog tick loop — runs alongside `garagetytus serve`.
/// Per LD#10, polls disk + integrity + (future) creds-migrate
/// every `TICK_INTERVAL_S` seconds and writes
/// `<state-dir>/watchdog.json` atomically.
fn spawn_watchdog_loop(
    state_dir: std::path::PathBuf,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let tick_interval = std::env::var("GARAGETYTUS_WATCHDOG_INTERVAL_S")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(garagetytus_watchdogs::TICK_INTERVAL_S);

    let started = chrono::Utc::now();
    let data_dir = state_dir.clone();
    let mut prev_mode = garagetytus_watchdogs::Mode::Rw;
    while !shutdown.load(Ordering::Relaxed) {
        match garagetytus_watchdogs::tick(&state_dir, &data_dir, prev_mode, started)
        {
            Ok(state) => {
                prev_mode = state.mode;
                tracing::debug!(
                    "watchdog tick: mode={} disk_free_pct={:.2}",
                    state.mode.as_str(),
                    state.disk_free_pct
                );
            }
            Err(e) => {
                tracing::warn!("watchdog tick error: {}", e);
            }
        }
        // Cooperative sleep — checks the shutdown flag every second
        // so Ctrl-C doesn't have to wait the full tick.
        for _ in 0..tick_interval {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

// ─── helpers ────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn plist_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", PLIST_LABEL))
}

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("systemd/user")
        .join(SERVICE_UNIT)
}

fn which_garage() -> std::path::PathBuf {
    // Linux install.rs drops garage into ~/.local/bin; macOS installs
    // it via brew which puts it on PATH. Try the common locations,
    // falling back to "garage" so PATH lookup still works.
    let candidates: Vec<std::path::PathBuf> = vec![
        dirs::home_dir().unwrap_or_default().join(".local/bin/garage"),
        std::path::PathBuf::from("/opt/homebrew/bin/garage"),
        std::path::PathBuf::from("/usr/local/bin/garage"),
    ];
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    std::path::PathBuf::from("garage")
}

/// Tiny libc shim — `geteuid()` for macOS launchctl domain. Avoids
/// pulling in the full `nix` crate for one syscall.
#[cfg(unix)]
unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_garage_returns_some_path() {
        let p = which_garage();
        assert!(!p.as_os_str().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_path_under_launchagents() {
        let p = plist_path();
        let s = p.display().to_string();
        assert!(s.contains("Library/LaunchAgents"));
        assert!(s.ends_with("com.traylinx.garagetytus.plist"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_unit_path_under_user_dir() {
        let p = systemd_unit_path();
        let s = p.display().to_string();
        assert!(s.contains("systemd/user"));
        assert!(s.ends_with("garagetytus.service"));
    }

    #[test]
    fn const_window_deferral_is_set() {
        // Just exercise the const so it's not flagged dead on
        // non-windows.
        let _ = WINDOWS_DEFERRAL;
    }
}
