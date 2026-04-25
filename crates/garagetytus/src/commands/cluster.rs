//! `garagetytus cluster {init,status,repair}` — v0.5 multinode
//! lifecycle. Q4-Q5-Q6 verdicts (lope round 2026-04-25) lock the
//! invocation surface; this module wires the locked CLI shape.
//!
//! **Phase 0 prereq.** Per the canonical sprint at
//! `MAKAKOO/development/sprints/queued/MAKAKOO-OS-V0.8-S3-CLUSTER/SPRINT.md`,
//! Phase 0 droplet probes (8 of them) gate Phase A. The probes
//! produce `results/PHASE-0-RESULTS.md` against a real droplet —
//! they cannot run from a chat session. Bash scripts to execute
//! the probes ship at
//! `garagetytus/sprint-v0.5/phase0/`.
//!
//! Until Phase 0 results are recorded, `cluster init` ships a
//! "preflight only" mode that:
//!   1. Validates the args.
//!   2. Generates an `rpc_secret` if absent.
//!   3. Writes `<config_dir>/cluster.toml` (atomic).
//!   4. Prints the SSH steps that Phase A.1 will execute once
//!      Phase 0 completes — but does NOT execute them.
//!
//! `cluster status` works against an existing config; `cluster
//! repair` is scaffold-only until Phase A.1 lands the SSH layer.
//! Both subcommands print clear "Phase 0 pending" messaging when
//! they detect the cluster has not actually been bootstrapped.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use garagetytus_core::{
    cluster_config_path, cluster_state_path, parse_config, serialize_config, ClusterConfig,
    ClusterState,
};

use crate::context::CliContext;

/// Generate a cryptographically-random 32-byte hex string for the
/// cluster RPC secret. Uses `OsRng` (CSPRNG); deliberately avoids
/// shelling to `openssl rand` so the binary works on bare hosts.
fn generate_rpc_secret() -> String {
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let mut out = String::with_capacity(64);
    for b in buf {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// `cluster init` — Phase A.1 preflight + config write. SSH-driven
/// droplet steps land once Phase 0 outcomes are recorded.
#[allow(clippy::too_many_arguments)]
pub fn init(
    _ctx: &CliContext,
    droplet_host: String,
    rpc_secret: Option<String>,
    mac_zone: Option<String>,
    droplet_zone: Option<String>,
    pod_endpoint: Option<String>,
    dry_run: bool,
    force: bool,
) -> Result<i32> {
    let cfg_path = cluster_config_path();
    if cfg_path.exists() && !force {
        eprintln!(
            "garagetytus cluster init: already initialized at {}",
            cfg_path.display()
        );
        eprintln!("  Re-run with --force to regenerate the config.");
        eprintln!("  Or use `garagetytus cluster status` to inspect.");
        return Ok(1);
    }

    let secret = match rpc_secret {
        Some(s) => s,
        None => generate_rpc_secret(),
    };
    let cfg = ClusterConfig::new(
        secret,
        droplet_host,
        mac_zone,
        droplet_zone,
        pod_endpoint,
    );
    cfg.validate().context("cluster config validation failed")?;

    println!("garagetytus cluster init: plan");
    println!("  config target: {}", cfg_path.display());
    println!("  mac zone:      {}", cfg.mac_zone);
    println!("  droplet zone:  {}", cfg.droplet_zone);
    println!("  droplet host:  {}", cfg.droplet_host);
    println!("  pod endpoint:  {}", cfg.pod_endpoint);
    println!("  rpc_secret:    {}…{} (64 chars)",
        &cfg.rpc_secret[..8], &cfg.rpc_secret[56..]);
    println!("  replication:   {}", cfg.replication_factor);

    if dry_run {
        println!();
        println!("garagetytus cluster init: --dry-run, no changes written.");
        return Ok(0);
    }

    let body = serialize_config(&cfg).context("serialize cluster.toml")?;
    if let Some(parent) = cfg_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    write_atomic(&cfg_path, body.as_bytes())
        .with_context(|| format!("write {}", cfg_path.display()))?;
    println!();
    println!("garagetytus cluster init: wrote {}", cfg_path.display());

    println!();
    println!("Next steps (Phase A.1 — pending Phase 0 droplet probes):");
    println!("  1. Run Phase 0 probes against the droplet:");
    println!("       bash sprint-v0.5/phase0/probe.sh {}",
        cfg.droplet_host);
    println!("  2. Commit results to");
    println!("       MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/results/PHASE-0-RESULTS.md");
    println!("  3. Re-run `garagetytus cluster init --force` once Phase A.1");
    println!("     SSH orchestration ships (the rpc_secret + binary push +");
    println!("     systemd unit installation are gated on the probe results).");
    println!();
    println!("Until Phase A.1 lands, the cluster.toml above is preflight-");
    println!("complete but the droplet remains uninitialized.");

    Ok(0)
}

/// `cluster status` — print the current cluster state. Reads the
/// optional `cluster_state.json` written by the watchdog tick (when
/// cluster mode is fully wired in Phase A.5) + falls back to
/// "preflight only" messaging when only `cluster.toml` exists.
pub fn status(_ctx: &CliContext, json: bool) -> Result<i32> {
    let cfg_path = cluster_config_path();
    if !cfg_path.exists() {
        if json {
            println!("{}", serde_json::json!({
                "status": "single_node",
                "message": "cluster.toml not present — single-node mode",
            }));
        } else {
            println!("garagetytus cluster status: single-node mode (cluster.toml not present)");
            println!("  Run `garagetytus cluster init --droplet-host <user@host>` to bootstrap.");
        }
        return Ok(0);
    }

    let body = fs::read_to_string(&cfg_path)
        .with_context(|| format!("read {}", cfg_path.display()))?;
    let cfg = parse_config(&body).context("parse cluster.toml")?;
    cfg.validate().context("cluster.toml failed validation")?;

    let state_path = cluster_state_path();
    let state: Option<ClusterState> = if state_path.exists() {
        let bytes = fs::read(&state_path)
            .with_context(|| format!("read {}", state_path.display()))?;
        Some(serde_json::from_slice(&bytes).context("parse cluster_state.json")?)
    } else {
        None
    };

    if json {
        println!("{}", serde_json::json!({
            "status": "cluster_configured",
            "config": {
                "mac_zone": cfg.mac_zone,
                "droplet_zone": cfg.droplet_zone,
                "droplet_host": cfg.droplet_host,
                "pod_endpoint": cfg.pod_endpoint,
                "replication_factor": cfg.replication_factor,
            },
            "runtime_state": state,
            "phase_a1_complete": state.is_some(),
        }));
        return Ok(0);
    }

    println!("garagetytus cluster status");
    println!("  config:   {}", cfg_path.display());
    println!("  zones:    {} + {}", cfg.mac_zone, cfg.droplet_zone);
    println!("  droplet:  {}", cfg.droplet_host);
    println!("  pod URL:  {}", cfg.pod_endpoint);
    println!("  rep_f:    {}", cfg.replication_factor);
    println!();
    if let Some(s) = state {
        println!("  layout version: {}", s.layout_version);
        for (zone, node) in &s.nodes {
            let reach = if node.reachable { "✓" } else { "✗" };
            let pct = node
                .disk_free_pct
                .map(|p| format!("{:.1}%", p))
                .unwrap_or_else(|| "?".to_string());
            println!(
                "  {} {:<10} disk_free={} last_heartbeat={}",
                reach,
                zone,
                pct,
                node.last_heartbeat_unix_seconds
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "never".to_string())
            );
        }
    } else {
        println!("  runtime state: not yet observed");
        println!("  (Phase A.5 watchdog tick lands cluster_state.json once");
        println!("   the cluster is bootstrapped + nodes are reachable.)");
    }

    Ok(0)
}

/// `cluster repair` — Phase B.4 orchestration scaffold. Will SSH
/// to each node and run `garage repair tables --yes` once Phase
/// A.1 SSH layer lands. For now, prints the plan + clear "Phase 0
/// pending" messaging.
pub fn repair(
    _ctx: &CliContext,
    nodes: Option<Vec<String>>,
    force: bool,
    dry_run: bool,
) -> Result<i32> {
    let cfg_path = cluster_config_path();
    if !cfg_path.exists() {
        eprintln!(
            "garagetytus cluster repair: cluster not initialized (no {})",
            cfg_path.display()
        );
        eprintln!("  Run `garagetytus cluster init --droplet-host <user@host>` first.");
        eprintln!("  For local-only repair, run `garagetytus repair`.");
        return Ok(1);
    }

    let body = fs::read_to_string(&cfg_path)
        .with_context(|| format!("read {}", cfg_path.display()))?;
    let cfg = parse_config(&body).context("parse cluster.toml")?;

    let target_nodes: Vec<String> = match nodes {
        Some(list) if !list.is_empty() => list,
        _ => vec![cfg.mac_zone.clone(), cfg.droplet_zone.clone()],
    };

    println!("garagetytus cluster repair: plan");
    for node in &target_nodes {
        let host = if node == &cfg.droplet_zone {
            cfg.droplet_host.as_str()
        } else {
            "(local)"
        };
        println!(
            "  {:<10} → ssh {} -- garage repair tables --yes{}",
            node,
            host,
            if force { " (force)" } else { "" }
        );
    }

    if dry_run {
        println!();
        println!("garagetytus cluster repair: --dry-run, no changes executed.");
        return Ok(0);
    }

    println!();
    println!("garagetytus cluster repair: SSH orchestration lands in Phase A.1");
    println!("  Until then, run `garagetytus repair` on each host directly:");
    println!("    garagetytus repair                # local node");
    println!("    ssh {} -- garagetytus repair    # droplet (when binary present)",
        cfg.droplet_host);
    println!();
    println!("  Per Q5 verdict (lope pi+codex), the local repair is per-node");
    println!("  + sub-second + idempotent; cluster anti-entropy reconciles.");

    Ok(0)
}

/// Atomic write — `<path>.tmp` → fsync → rename. Same pattern as
/// `garagetytus-watchdogs::write_watchdog_json`.
fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("path {} has no parent", path.display()))?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("cluster.toml")
    ));
    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("create {}", tmp.display()))?;
        use std::io::Write;
        f.write_all(body)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// `garagetytus repair` — single-node repair entry point. Wraps
/// `garage repair tables --yes` against the local config. Exists
/// alongside `cluster repair` so users in single-node mode never
/// have to think about cluster topology.
pub fn local_repair(_ctx: &CliContext) -> Result<i32> {
    use std::process::Command;
    let cfg = garagetytus_core::paths::config_dir().join("garagetytus.toml");
    if !cfg.exists() {
        eprintln!(
            "garagetytus repair: config missing at {} — run `garagetytus install` first.",
            cfg.display()
        );
        return Ok(1);
    }
    let garage_bin = locate_garage();
    println!(
        "garagetytus repair: shelling `{} -c {} repair tables --yes`",
        garage_bin.display(),
        cfg.display()
    );
    let status = Command::new(&garage_bin)
        .arg("-c")
        .arg(&cfg)
        .args(["repair", "tables", "--yes"])
        .status()?;
    if !status.success() {
        bail!(
            "garage repair tables failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    println!("garagetytus repair: done.");
    Ok(0)
}

fn locate_garage() -> PathBuf {
    let candidates: Vec<PathBuf> = vec![
        dirs::home_dir().unwrap_or_default().join(".local/bin/garage"),
        PathBuf::from("/opt/homebrew/bin/garage"),
        PathBuf::from("/usr/local/bin/garage"),
    ];
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    PathBuf::from("garage")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_rpc_secret_is_64_hex_chars() {
        let s = generate_rpc_secret();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_rpc_secret_is_random() {
        // Vanishingly small probability of collision (256 bits).
        let a = generate_rpc_secret();
        let b = generate_rpc_secret();
        assert_ne!(a, b);
    }

    #[test]
    fn write_atomic_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("hello.txt");
        write_atomic(&target, b"hello world").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello world");
    }

    #[test]
    fn write_atomic_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("hello.txt");
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    }
}
