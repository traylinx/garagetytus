//! `garagetytus sync *` — Mac-client shared-folder sync maintenance (Phase 3).
//!
//! Two responsibilities, both client-side (CONFIRMED fact #6: there is no
//! server registry of users' rclone endpoints, so endpoint convergence MUST be
//! self-heal on the client):
//!
//!   * `heal-endpoint` — converge a STALE ephemeral per-pod Garage endpoint
//!     (`http://10.<octet>.<n>.1:3900`, orphaned when a pod is reallocated) to
//!     the reallocation-stable `http://10.42.42.1:3900`. The rewrite is done by
//!     a section-aware parse of `rclone.conf` (NOT a blind regex over the whole
//!     file): it touches ONLY the `[garagetytus]` remote's `endpoint`, leaves
//!     `tytusaws`/comments/other remotes byte-for-byte intact, backs up first,
//!     writes atomically, and is idempotent (already-stable = no-op).
//!
//!   * `health` — probe the configured endpoint and emit the schema-versioned
//!     `mac-sync-health-v1` file the tytus-cli daemon reads and the tytus-os UI
//!     renders. Consumers treat stale/missing/malformed as not-trusted and
//!     never as "synced".

use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use serde_json::json;

use crate::context::CliContext;

/// Reallocation-stable Garage endpoint exposed on every droplet via the
/// 3-hop socat proxy chain. A *different service* from the `:18080` LLM gateway.
pub const STABLE_ENDPOINT: &str = "http://10.42.42.1:3900";
/// The only rclone remote garagetytus owns. Never touch other remotes.
const REMOTE: &str = "garagetytus";
const SYNC_HEALTH_SCHEMA: &str = "mac-sync-health-v1";
const DEFAULT_STALE_AFTER_SECONDS: u64 = 300;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn default_rclone_path() -> PathBuf {
    // rclone uses ~/.config/rclone/rclone.conf even on macOS (matches
    // garagetytus-folder-bind), NOT dirs::config_dir() which is ~/Library on mac.
    home().join(".config").join("rclone").join("rclone.conf")
}

fn default_health_path() -> PathBuf {
    home()
        .join(".cache")
        .join("garagetytus")
        .join("sync-health.json")
}

fn state_path_for(out: &Path) -> PathBuf {
    out.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("sync-health.state.json")
}

// ---------------------------------------------------------------------------
// endpoint classification (value-level; the rewrite itself is structural)
// ---------------------------------------------------------------------------

fn endpoint_host(ep: &str) -> Option<String> {
    let after = ep.split("://").nth(1)?;
    let hostport = after.split('/').next()?;
    let host = match hostport.rsplit_once(':') {
        Some((h, _)) => h,
        None => hostport,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn endpoint_port(ep: &str) -> Option<String> {
    let after = ep.split("://").nth(1)?;
    let hostport = after.split('/').next()?;
    hostport.rsplit_once(':').map(|(_, p)| p.to_string())
}

/// True iff `ep` is an ephemeral per-pod sidecar gateway `10.<a>.<b>.1:3900`
/// and NOT the stable `10.42.42.1`. This is the only shape we rewrite.
fn is_ephemeral_endpoint(ep: &str) -> bool {
    let host = match endpoint_host(ep) {
        Some(h) => h,
        None => return false,
    };
    if host == "10.42.42.1" {
        return false; // the stable target — never "stale"
    }
    if endpoint_port(ep).as_deref() != Some("3900") {
        return false; // garage S3 port; anything else is not our ephemeral shape
    }
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts[0] == "10"
        && parts[3] == "1"
        && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

// ---------------------------------------------------------------------------
// structured rclone.conf rewrite (section-aware, formatting-preserving)
// ---------------------------------------------------------------------------

struct HealOutcome {
    previous: Option<String>,
    sections_rewritten: usize,
}

/// Rewrite the `endpoint` of the `[garagetytus]` remote(s) to `stable` IFF the
/// current value is an ephemeral per-pod endpoint. Everything else — other
/// sections, comments, blank lines, key order — is preserved exactly.
fn rewrite_garagetytus_endpoint(text: &str, stable: &str) -> (String, HealOutcome) {
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_target = false;
    let mut previous = None;
    let mut count = 0usize;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = body.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
            let name = trimmed[1..trimmed.len() - 1].trim();
            in_target = name == REMOTE;
            out.push_str(line);
            continue;
        }
        if in_target && !trimmed.starts_with('#') && !trimmed.starts_with(';') {
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == "endpoint" && is_ephemeral_endpoint(v.trim()) {
                    previous = Some(v.trim().to_string());
                    count += 1;
                    let nl = if line.ends_with('\n') { "\n" } else { "" };
                    out.push_str(&format!("endpoint = {}{}", stable, nl));
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    (
        out,
        HealOutcome {
            previous,
            sections_rewritten: count,
        },
    )
}

fn read_garagetytus_endpoint(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_target = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
            in_target = t[1..t.len() - 1].trim() == REMOTE;
            continue;
        }
        if in_target {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == "endpoint" {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn set_mode_600(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn set_mode_600(_p: &Path) {}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "rclone.conf".into());
    path.with_file_name(format!("{}.bak.{}", name, stamp))
}

// ---------------------------------------------------------------------------
// subcommand: heal-endpoint
// ---------------------------------------------------------------------------

pub fn heal_endpoint(
    _ctx: &CliContext,
    config: Option<String>,
    stable_endpoint: Option<String>,
    dry_run: bool,
    json_out: bool,
) -> Result<i32> {
    let path = config
        .map(PathBuf::from)
        .unwrap_or_else(default_rclone_path);
    let stable = stable_endpoint.unwrap_or_else(|| STABLE_ENDPOINT.to_string());

    let mut result = json!({
        "action": "noop",
        "config": path.display().to_string(),
        "remote": REMOTE,
        "stable_endpoint": stable,
        "previous_endpoint": serde_json::Value::Null,
        "sections_rewritten": 0,
        "dry_run": dry_run,
    });

    if !path.exists() {
        result["action"] = json!("no_config");
        return finish(json_out, &result, "no rclone.conf; nothing to heal");
    }

    let text = std::fs::read_to_string(&path)?;
    let (new_text, outcome) = rewrite_garagetytus_endpoint(&text, &stable);
    result["previous_endpoint"] = match &outcome.previous {
        Some(p) => json!(p),
        None => serde_json::Value::Null,
    };
    result["sections_rewritten"] = json!(outcome.sections_rewritten);

    if outcome.sections_rewritten == 0 {
        return finish(
            json_out,
            &result,
            "[garagetytus] endpoint already stable / not ephemeral; no-op",
        );
    }
    if dry_run {
        result["action"] = json!("would_heal");
        return finish(json_out, &result, "would rewrite stale endpoint (dry-run)");
    }

    let backup = backup_path(&path);
    std::fs::copy(&path, &backup).ok();
    let tmp = path.with_file_name(format!(
        "{}.heal.{}",
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "rclone.conf".into()),
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(new_text.as_bytes())?;
        let _ = f.sync_all();
    }
    set_mode_600(&tmp);
    std::fs::rename(&tmp, &path)?;
    set_mode_600(&path);

    result["action"] = json!("healed");
    result["backup"] = json!(backup.display().to_string());
    finish(
        json_out,
        &result,
        "rewrote stale ephemeral endpoint -> stable",
    )
}

fn finish(json_out: bool, result: &serde_json::Value, human: &str) -> Result<i32> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(result)?);
    } else {
        println!("{} ({})", human, result["action"].as_str().unwrap_or("?"));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// subcommand: health (sync-health producer)
// ---------------------------------------------------------------------------

fn probe_reachable(ep: &str, timeout: Duration) -> bool {
    let host = match endpoint_host(ep) {
        Some(h) => h,
        None => return false,
    };
    let port = endpoint_port(ep).unwrap_or_else(|| "3900".to_string());
    let addr = format!("{}:{}", host, port);
    match addr.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => TcpStream::connect_timeout(&a, timeout).is_ok(),
            None => false,
        },
        Err(_) => false,
    }
}

fn load_state(path: &Path) -> (u64, Option<String>) {
    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(_) => return (0, None),
    };
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    let cf = v
        .get("consecutive_failures")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let last = v
        .get("last_success_ts")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    (cf, last)
}

pub fn health(
    _ctx: &CliContext,
    out: Option<String>,
    config: Option<String>,
    stale_after_seconds: u64,
    json_out: bool,
) -> Result<i32> {
    let cfg = config
        .map(PathBuf::from)
        .unwrap_or_else(default_rclone_path);
    let out_path = out.map(PathBuf::from).unwrap_or_else(default_health_path);
    let stale_after = if stale_after_seconds == 0 {
        DEFAULT_STALE_AFTER_SECONDS
    } else {
        stale_after_seconds
    };

    let endpoint = read_garagetytus_endpoint(&cfg).unwrap_or_else(|| STABLE_ENDPOINT.to_string());
    let reachable = probe_reachable(&endpoint, Duration::from_secs(3));

    let state_path = state_path_for(&out_path);
    let (prev_failures, prev_success) = load_state(&state_path);
    let now = Utc::now();
    let now_iso = now.to_rfc3339_opts(SecondsFormat::Secs, true);

    let (consecutive_failures, last_success_ts, last_error) = if reachable {
        (0u64, Some(now_iso.clone()), serde_json::Value::Null)
    } else {
        (
            prev_failures + 1,
            prev_success,
            json!(format!(
                "endpoint {} unreachable (TCP connect failed)",
                endpoint
            )),
        )
    };
    let state = if reachable {
        "ok"
    } else if consecutive_failures >= 3 {
        "failed"
    } else {
        "degraded"
    };

    let payload = json!({
        "schema_version": SYNC_HEALTH_SCHEMA,
        "updated_at": now_iso,
        "stale_after_seconds": stale_after,
        "endpoint_checked": endpoint,
        "reachable": reachable,
        "last_success_ts": match &last_success_ts { Some(s) => json!(s), None => serde_json::Value::Null },
        "consecutive_failures": consecutive_failures,
        "last_error": last_error,
        "state": state,
        "excluded": json!({}),
        "bindings": json!([]),
    });

    if let Some(dir) = out_path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    // atomic write of the public health file
    let tmp = out_path.with_file_name(format!(
        "{}.tmp.{}",
        out_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "sync-health.json".into()),
        std::process::id()
    ));
    std::fs::write(&tmp, serde_json::to_string_pretty(&payload)? + "\n")?;
    std::fs::rename(&tmp, &out_path)?;

    // persist the failure counter for the next probe
    let _ = std::fs::write(
        &state_path,
        serde_json::to_string(&json!({
            "consecutive_failures": consecutive_failures,
            "last_success_ts": match &last_success_ts { Some(s) => json!(s), None => serde_json::Value::Null },
        }))? + "\n",
    );

    if json_out {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "sync-health: state={} reachable={} endpoint={} -> {}",
            state,
            reachable,
            endpoint,
            out_path.display()
        );
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    const SAMPLE: &str = "\
# my rclone config
[garagetytus]
type = s3
provider = Other
access_key_id = GKEXAMPLE
secret_access_key = secretvalue
endpoint = http://10.18.2.1:3900
region = garage

[tytusaws]
type = s3
endpoint = http://10.18.9.1:3900
region = us-east-1
";

    #[test]
    fn detects_ephemeral_vs_stable() {
        assert!(is_ephemeral_endpoint("http://10.18.2.1:3900"));
        assert!(is_ephemeral_endpoint("http://10.5.7.1:3900/"));
        assert!(!is_ephemeral_endpoint("http://10.42.42.1:3900")); // stable
        assert!(!is_ephemeral_endpoint("http://127.0.0.1:3900"));
        assert!(!is_ephemeral_endpoint("http://10.18.2.2:3900")); // not .1
        assert!(!is_ephemeral_endpoint("http://10.18.2.1:9000")); // not garage port
        assert!(!is_ephemeral_endpoint("https://s3.amazonaws.com"));
    }

    #[test]
    fn rewrites_only_garagetytus_endpoint() {
        let (out, outcome) = rewrite_garagetytus_endpoint(SAMPLE, STABLE_ENDPOINT);
        assert_eq!(outcome.sections_rewritten, 1);
        assert_eq!(outcome.previous.as_deref(), Some("http://10.18.2.1:3900"));
        // garagetytus endpoint healed
        assert!(out.contains("endpoint = http://10.42.42.1:3900"));
        // tytusaws endpoint UNTOUCHED (it is not ours, even though it looks ephemeral)
        assert!(out.contains("endpoint = http://10.18.9.1:3900"));
        // comment + keys preserved
        assert!(out.contains("# my rclone config"));
        assert!(out.contains("access_key_id = GKEXAMPLE"));
        assert!(out.contains("region = us-east-1"));
    }

    #[test]
    fn idempotent_second_pass_is_noop() {
        let (once, _) = rewrite_garagetytus_endpoint(SAMPLE, STABLE_ENDPOINT);
        let (twice, outcome) = rewrite_garagetytus_endpoint(&once, STABLE_ENDPOINT);
        assert_eq!(outcome.sections_rewritten, 0);
        assert_eq!(once, twice);
    }

    #[test]
    fn already_stable_is_noop() {
        let stable_cfg = "[garagetytus]\nendpoint = http://10.42.42.1:3900\n";
        let (_, outcome) = rewrite_garagetytus_endpoint(stable_cfg, STABLE_ENDPOINT);
        assert_eq!(outcome.sections_rewritten, 0);
    }

    #[test]
    fn no_garagetytus_section_is_noop() {
        let cfg = "[tytusaws]\nendpoint = http://10.18.2.1:3900\n";
        let (out, outcome) = rewrite_garagetytus_endpoint(cfg, STABLE_ENDPOINT);
        assert_eq!(outcome.sections_rewritten, 0);
        assert_eq!(out, cfg);
    }

    #[test]
    fn heal_endpoint_writes_backup_and_atomic_swap() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rclone.conf");
        std::fs::write(&cfg, SAMPLE).unwrap();
        let ctx = CliContext::new().unwrap();
        let code = heal_endpoint(&ctx, Some(cfg.display().to_string()), None, false, true).unwrap();
        assert_eq!(code, 0);
        let healed = std::fs::read_to_string(&cfg).unwrap();
        assert!(healed.contains("endpoint = http://10.42.42.1:3900"));
        assert!(healed.contains("endpoint = http://10.18.9.1:3900")); // tytusaws intact
                                                                      // a timestamped backup exists
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn heal_endpoint_missing_config_is_noop() {
        let ctx = CliContext::new().unwrap();
        let code = heal_endpoint(
            &ctx,
            Some("/nonexistent/rclone.conf".into()),
            None,
            true,
            true,
        )
        .unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn health_reports_failed_when_unreachable() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rclone.conf");
        // port 1 on loopback is closed -> fast connection refusal
        std::fs::write(&cfg, "[garagetytus]\nendpoint = http://127.0.0.1:1\n").unwrap();
        let out = dir.path().join("sync-health.json");
        let ctx = CliContext::new().unwrap();
        for _ in 0..3 {
            health(
                &ctx,
                Some(out.display().to_string()),
                Some(cfg.display().to_string()),
                300,
                false,
            )
            .unwrap();
        }
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["schema_version"], "mac-sync-health-v1");
        assert_eq!(v["reachable"], false);
        assert!(v["consecutive_failures"].as_u64().unwrap() >= 3);
        assert_eq!(v["state"], "failed");
        assert!(v["endpoint_checked"]
            .as_str()
            .unwrap()
            .contains("127.0.0.1:1"));
    }

    #[test]
    fn health_reports_ok_when_reachable() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rclone.conf");
        std::fs::write(
            &cfg,
            format!("[garagetytus]\nendpoint = http://127.0.0.1:{}\n", port),
        )
        .unwrap();
        let out = dir.path().join("sync-health.json");
        let ctx = CliContext::new().unwrap();
        health(
            &ctx,
            Some(out.display().to_string()),
            Some(cfg.display().to_string()),
            300,
            false,
        )
        .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["reachable"], true);
        assert_eq!(v["state"], "ok");
        assert_eq!(v["consecutive_failures"], 0);
        assert!(v["last_success_ts"].is_string());
    }
}
