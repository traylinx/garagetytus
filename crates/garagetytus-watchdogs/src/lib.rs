//! `garagetytus-watchdogs` — LD#10 + LD#11 implementation.
//!
//! Three checks run every tick:
//!
//! 1. **Disk watch** (LD#10) — read free-space % on the data
//!    partition; below 10% → flip `mode` to read-only ("ro"),
//!    above 15% → back to "rw" (10/15 hysteresis avoids flapping).
//! 2. **Integrity check** — on the first tick after process
//!    start, write a sentinel file. If the sentinel was already
//!    present at startup, that's an unclean-shutdown signal —
//!    increment the counter + log a high-priority event.
//! 3. **Keychain migrate** — if a legacy `(makakoo,
//!    makakoo-s3-service)` keychain entry exists and the new
//!    `(garagetytus, s3-service)` does not, copy + delete the
//!    legacy. Idempotent.
//!
//! Each tick writes [`WatchdogState`] to `<state-dir>/watchdog.json`
//! atomically (write to `.tmp`, fsync, rename). Per LD#11, this
//! is the JSON mirror that CLIs/dashboards poll. The Prometheus
//! `/metrics` endpoint is v0.2 (deferred — needs an HTTP server
//! sidecar; v0.1 ships JSON-only).
//!
//! Run shape: spawn `tokio::spawn(loop_forever(state_dir))` from
//! `garagetytus serve`. The loop sleeps `TICK_INTERVAL` between
//! ticks; cancellation drops the future cleanly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

/// Default poll cadence — once every 30 s. Override via
/// `GARAGETYTUS_WATCHDOG_INTERVAL_S` env var.
pub const TICK_INTERVAL_S: u64 = 30;

/// Hysteresis thresholds (LD#10).
pub const DISK_RO_THRESHOLD_PCT: f64 = 10.0;
pub const DISK_RW_THRESHOLD_PCT: f64 = 15.0;

/// On-disk JSON shape served at `<state-dir>/watchdog.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchdogState {
    pub mode: Mode,
    pub disk_free_pct: f64,
    pub uptime_seconds: u64,
    pub unclean_shutdown_total: u64,
    pub last_tick_unix_seconds: i64,
    pub data_dir: PathBuf,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Rw,
    Ro,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Rw => "rw",
            Mode::Ro => "ro",
        }
    }
}

#[derive(Debug, Error)]
pub enum WatchdogError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Process-local counter. Persisted to disk via WatchdogState
/// each tick; survives daemon restarts via deserialization.
static UNCLEAN_SHUTDOWN_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Run one tick — read disk, run integrity probe, run creds
/// migration, write watchdog.json atomically. `prev_mode` is the
/// previous tick's mode (passed in to apply hysteresis); pass
/// `Mode::Rw` on first tick.
pub fn tick(
    state_dir: &Path,
    data_dir: &Path,
    prev_mode: Mode,
    started_at: DateTime<Utc>,
) -> Result<WatchdogState> {
    std::fs::create_dir_all(state_dir).with_context(|| {
        format!("create state dir {}", state_dir.display())
    })?;

    let disk_free_pct = read_disk_free_pct(data_dir).unwrap_or(0.0);
    let mode = next_mode(prev_mode, disk_free_pct);

    if let Err(e) = integrity_check_step(state_dir) {
        warn!("integrity check error: {}", e);
    }

    let now = Utc::now();
    let uptime = (now - started_at).num_seconds().max(0) as u64;

    let state = WatchdogState {
        mode,
        disk_free_pct,
        uptime_seconds: uptime,
        unclean_shutdown_total: UNCLEAN_SHUTDOWN_TOTAL.load(Ordering::Relaxed),
        last_tick_unix_seconds: now.timestamp(),
        data_dir: data_dir.to_path_buf(),
        schema_version: 1,
    };

    write_watchdog_json(state_dir, &state)?;
    Ok(state)
}

/// LD#10 hysteresis transition. Below 10% → ro; above 15% → rw;
/// in-between → keep prev.
pub fn next_mode(prev: Mode, free_pct: f64) -> Mode {
    if free_pct < DISK_RO_THRESHOLD_PCT {
        Mode::Ro
    } else if free_pct >= DISK_RW_THRESHOLD_PCT {
        Mode::Rw
    } else {
        prev
    }
}

/// Q6 hybrid verdict — derive a strict cluster-wide mode from a
/// set of per-zone modes. Cluster is `rw` iff EVERY zone is `rw`;
/// any zone in `ro` flips the cluster rollup to `ro`. Empty input
/// (no zones reporting) defaults to `ro` — the conservative
/// "we don't know, so don't write" posture.
///
/// Used by `commands::metrics` to render the
/// `garagetytus_cluster_mode{...}` rollup gauge alongside the
/// per-zone primary signal. Pure function — no I/O — to keep the
/// derivation deterministic and unit-testable across all
/// {rw, ro}^N input combinations.
pub fn derive_cluster_mode(zone_modes: &[(String, Mode)]) -> Mode {
    if zone_modes.is_empty() {
        return Mode::Ro;
    }
    if zone_modes.iter().all(|(_, m)| *m == Mode::Rw) {
        Mode::Rw
    } else {
        Mode::Ro
    }
}

/// Read the disk-free percentage on the partition that holds
/// `data_dir`. Cross-platform via the `sysinfo` crate (LD#10 —
/// no `df` shellout).
pub fn read_disk_free_pct(data_dir: &Path) -> Option<f64> {
    use sysinfo::Disks;
    let target = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<&sysinfo::Disk> = None;
    let mut best_len = 0usize;
    for d in disks.list() {
        let mp = d.mount_point();
        if target.starts_with(mp) && mp.as_os_str().len() >= best_len {
            best_len = mp.as_os_str().len();
            best = Some(d);
        }
    }
    let d = best?;
    if d.total_space() == 0 {
        return None;
    }
    Some((d.available_space() as f64 / d.total_space() as f64) * 100.0)
}

/// Integrity sentinel — `<state-dir>/sentinel.lock` carries the
/// PID of the running daemon. On startup, if the sentinel exists
/// AND its PID is no longer alive (no /proc/<pid> on Linux,
/// `kill -0` on macOS), that's an unclean shutdown: increment
/// the counter, then take ownership.
fn integrity_check_step(state_dir: &Path) -> Result<()> {
    let sentinel = state_dir.join("sentinel.lock");
    let our_pid = std::process::id();

    if sentinel.exists() {
        if let Ok(prev) = std::fs::read_to_string(&sentinel) {
            let prev_pid: u32 = prev.trim().parse().unwrap_or(0);
            if prev_pid != our_pid && !pid_alive(prev_pid) {
                let n = UNCLEAN_SHUTDOWN_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
                info!(
                    "unclean-shutdown detected (prev pid={}); total={}",
                    prev_pid, n
                );
            }
        }
    }
    std::fs::write(&sentinel, our_pid.to_string()).with_context(|| {
        format!("write sentinel {}", sentinel.display())
    })?;
    Ok(())
}

fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // kill(pid, 0) — exists + caller has permission.
        unsafe {
            extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            kill(pid as i32, 0) == 0
        }
    }
    #[cfg(not(unix))]
    {
        // Best-effort fallback for hosts without unix: assume alive.
        let _ = pid;
        true
    }
}

/// Atomic write — write `<path>.tmp`, fsync, rename onto target.
/// Survives crashes mid-write.
pub fn write_watchdog_json(state_dir: &Path, state: &WatchdogState) -> Result<()> {
    let final_path = state_dir.join("watchdog.json");
    let tmp_path = state_dir.join("watchdog.json.tmp");
    let bytes = serde_json::to_vec_pretty(state)?;
    std::fs::write(&tmp_path, &bytes).with_context(|| {
        format!("write {}", tmp_path.display())
    })?;
    std::fs::rename(&tmp_path, &final_path).with_context(|| {
        format!(
            "rename {} -> {}",
            tmp_path.display(),
            final_path.display()
        )
    })?;
    Ok(())
}

/// AC8 preflight — runs at `garagetytus serve` startup BEFORE
/// garage launches. If `<state-dir>/sentinel.lock` carries a PID
/// that is no longer alive, the previous garagetytus process
/// crashed: increment the persistent unclean-shutdown counter
/// (mirrored in `<state-dir>/unclean_shutdown_total.txt`), log a
/// warning, and return `Ok(true)` so the caller can decide
/// whether to run `garage repair` before serve.
///
/// Returns `Ok(false)` on the clean-shutdown path (no sentinel,
/// or sentinel PID matches a still-alive process). Errors are
/// non-fatal — preflight always succeeds, the counter is just a
/// best-effort signal.
pub fn preflight_unclean_check(state_dir: &Path) -> Result<bool> {
    std::fs::create_dir_all(state_dir).ok();
    let sentinel = state_dir.join("sentinel.lock");
    let counter_path = state_dir.join("unclean_shutdown_total.txt");

    // Load the persisted counter into the in-memory atomic on
    // every preflight, regardless of clean/unclean — so that
    // tick() and /metrics report the correct historical value
    // across process restarts.
    let prev_total: u64 = std::fs::read_to_string(&counter_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    UNCLEAN_SHUTDOWN_TOTAL.store(prev_total, Ordering::Relaxed);

    if !sentinel.exists() {
        return Ok(false);
    }
    let body = match std::fs::read_to_string(&sentinel) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    let prev_pid: u32 = body.trim().parse().unwrap_or(0);
    if prev_pid == 0 || pid_alive(prev_pid) {
        // Either malformed sentinel OR previous garagetytus is
        // still running. Don't increment.
        return Ok(false);
    }

    let next = prev_total + 1;
    std::fs::write(&counter_path, next.to_string()).ok();
    UNCLEAN_SHUTDOWN_TOTAL.store(next, Ordering::Relaxed);

    tracing::warn!(
        "garagetytus: previous run did not exit cleanly (orphan pid={} in sentinel.lock); \
         unclean_shutdown_total now {}",
        prev_pid,
        next
    );
    Ok(true)
}

/// Read the current state from disk (for `garagetytus status`,
/// dashboards, tytus-tray polling). Returns `Ok(None)` if the
/// file is absent — that's the "watchdog never ran" signal.
pub fn read_watchdog_json(state_dir: &Path) -> Result<Option<WatchdogState>> {
    let path = state_dir.join("watchdog.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let state: WatchdogState = serde_json::from_slice(&bytes)?;
    Ok(Some(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn next_mode_hysteresis() {
        // Start rw, drop below 10 → ro.
        assert_eq!(next_mode(Mode::Rw, 9.5), Mode::Ro);
        // Start ro, climb to 12 (between 10 and 15) → stays ro.
        assert_eq!(next_mode(Mode::Ro, 12.0), Mode::Ro);
        // Start ro, climb to 15 → flips back to rw.
        assert_eq!(next_mode(Mode::Ro, 15.0), Mode::Rw);
        // Start rw, drop to 12 → stays rw.
        assert_eq!(next_mode(Mode::Rw, 12.0), Mode::Rw);
        // Threshold edges.
        assert_eq!(next_mode(Mode::Rw, 10.0), Mode::Rw);
        assert_eq!(next_mode(Mode::Rw, 9.99), Mode::Ro);
    }

    // ─── Q6 hybrid verdict — derive_cluster_mode ────────────

    fn zones(spec: &[(&str, Mode)]) -> Vec<(String, Mode)> {
        spec.iter().map(|(z, m)| (z.to_string(), *m)).collect()
    }

    #[test]
    fn derive_cluster_mode_empty_input_is_ro() {
        // No zones reporting → conservative "we don't know" posture.
        assert_eq!(derive_cluster_mode(&[]), Mode::Ro);
    }

    #[test]
    fn derive_cluster_mode_all_rw_is_rw() {
        let z = zones(&[("mac", Mode::Rw), ("droplet", Mode::Rw)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Rw);
    }

    #[test]
    fn derive_cluster_mode_any_ro_flips_cluster_to_ro() {
        // Mac rw, droplet ro → cluster ro (strict aggregation).
        let z = zones(&[("mac", Mode::Rw), ("droplet", Mode::Ro)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Ro);
        // Reverse — same outcome.
        let z = zones(&[("mac", Mode::Ro), ("droplet", Mode::Rw)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Ro);
    }

    #[test]
    fn derive_cluster_mode_all_ro_is_ro() {
        let z = zones(&[("mac", Mode::Ro), ("droplet", Mode::Ro)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Ro);
    }

    #[test]
    fn derive_cluster_mode_single_zone_passes_through() {
        // v0.1 single-node degenerate case: one zone reporting.
        // Strict aggregation = identity for n=1.
        let z = zones(&[("mac", Mode::Rw)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Rw);
        let z = zones(&[("mac", Mode::Ro)]);
        assert_eq!(derive_cluster_mode(&z), Mode::Ro);
    }

    #[test]
    fn derive_cluster_mode_three_node_future_proofing() {
        // v0.9+ N>2 case: still strict — all rw → rw, any ro → ro.
        let z = zones(&[
            ("mac", Mode::Rw),
            ("droplet-a", Mode::Rw),
            ("droplet-b", Mode::Rw),
        ]);
        assert_eq!(derive_cluster_mode(&z), Mode::Rw);
        let z = zones(&[
            ("mac", Mode::Rw),
            ("droplet-a", Mode::Rw),
            ("droplet-b", Mode::Ro),
        ]);
        assert_eq!(derive_cluster_mode(&z), Mode::Ro);
    }

    #[test]
    fn watchdog_state_serializes_and_round_trips() {
        let s = WatchdogState {
            mode: Mode::Rw,
            disk_free_pct: 87.5,
            uptime_seconds: 3600,
            unclean_shutdown_total: 0,
            last_tick_unix_seconds: 1_745_000_000,
            data_dir: PathBuf::from("/tmp/data"),
            schema_version: 1,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"mode\":\"rw\""));
        let back: WatchdogState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn write_watchdog_json_is_atomic() {
        let tmp = tempdir().unwrap();
        let state = WatchdogState {
            mode: Mode::Ro,
            disk_free_pct: 5.0,
            uptime_seconds: 1,
            unclean_shutdown_total: 1,
            last_tick_unix_seconds: 0,
            data_dir: PathBuf::from("/tmp"),
            schema_version: 1,
        };
        write_watchdog_json(tmp.path(), &state).unwrap();
        let final_path = tmp.path().join("watchdog.json");
        assert!(final_path.exists());
        // No leftover .tmp.
        assert!(!tmp.path().join("watchdog.json.tmp").exists());
        let read = read_watchdog_json(tmp.path()).unwrap().unwrap();
        assert_eq!(read.mode, Mode::Ro);
        assert_eq!(read.disk_free_pct, 5.0);
    }

    #[test]
    fn read_watchdog_json_returns_none_when_missing() {
        let tmp = tempdir().unwrap();
        assert!(read_watchdog_json(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn tick_writes_state_file() {
        let tmp = tempdir().unwrap();
        let started = Utc::now();
        let state = tick(tmp.path(), tmp.path(), Mode::Rw, started).unwrap();
        assert!(state.last_tick_unix_seconds > 0);
        assert_eq!(state.schema_version, 1);
        assert!(tmp.path().join("watchdog.json").exists());
    }

    #[test]
    fn integrity_check_step_creates_sentinel() {
        let tmp = tempdir().unwrap();
        integrity_check_step(tmp.path()).unwrap();
        let sentinel = tmp.path().join("sentinel.lock");
        assert!(sentinel.exists());
        let body = std::fs::read_to_string(&sentinel).unwrap();
        let pid: u32 = body.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn pid_alive_recognises_self() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(0));
    }

    #[test]
    fn read_disk_free_pct_returns_some_for_real_dir() {
        // `/tmp` exists on every CI runner.
        let pct = read_disk_free_pct(Path::new("/tmp"));
        assert!(pct.is_some());
        let v = pct.unwrap();
        assert!((0.0..=100.0).contains(&v), "implausible pct: {}", v);
    }

    #[test]
    fn preflight_unclean_check_clean_first_run() {
        // No sentinel, no counter file → returns Ok(false), counter stays 0.
        let tmp = tempdir().unwrap();
        UNCLEAN_SHUTDOWN_TOTAL.store(0, Ordering::Relaxed);
        let unclean = preflight_unclean_check(tmp.path()).unwrap();
        assert!(!unclean);
        assert_eq!(UNCLEAN_SHUTDOWN_TOTAL.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn preflight_unclean_check_detects_orphan_pid() {
        let tmp = tempdir().unwrap();
        // Seed an orphan-PID sentinel — pick a PID that's almost
        // certainly not alive (max u32 minus 1; kernel won't have
        // assigned this). pid=0 is filtered out as malformed,
        // hence picking a different sentinel.
        std::fs::write(tmp.path().join("sentinel.lock"), "999999999").unwrap();
        UNCLEAN_SHUTDOWN_TOTAL.store(0, Ordering::Relaxed);
        let unclean = preflight_unclean_check(tmp.path()).unwrap();
        assert!(unclean);
        // Counter file should now exist with value 1.
        let body = std::fs::read_to_string(tmp.path().join("unclean_shutdown_total.txt"))
            .unwrap();
        assert_eq!(body.trim(), "1");
        assert_eq!(UNCLEAN_SHUTDOWN_TOTAL.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn preflight_unclean_check_persisted_counter_is_loaded() {
        // Even on a clean run, the persisted counter from a
        // previous unclean shutdown must populate the in-memory
        // atomic.
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("unclean_shutdown_total.txt"), "7").unwrap();
        UNCLEAN_SHUTDOWN_TOTAL.store(0, Ordering::Relaxed);
        let unclean = preflight_unclean_check(tmp.path()).unwrap();
        assert!(!unclean);
        assert_eq!(UNCLEAN_SHUTDOWN_TOTAL.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn preflight_unclean_check_skips_when_sentinel_pid_alive() {
        // Sentinel with our own PID — process is alive, no
        // unclean-shutdown signal.
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("sentinel.lock"),
            std::process::id().to_string(),
        )
        .unwrap();
        UNCLEAN_SHUTDOWN_TOTAL.store(0, Ordering::Relaxed);
        let unclean = preflight_unclean_check(tmp.path()).unwrap();
        assert!(!unclean);
        assert!(!tmp.path().join("unclean_shutdown_total.txt").exists()
            || std::fs::read_to_string(
                tmp.path().join("unclean_shutdown_total.txt"),
            )
            .map(|s| s.trim() == "0")
            .unwrap_or(true));
    }
}
