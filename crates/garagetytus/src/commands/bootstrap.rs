//! `garagetytus bootstrap` — Phase B daemon first-run setup.
//!
//! Carved from `makakoo-os/makakoo/src/commands/s3.rs::bootstrap` body
//! (Phase A.2). Steps:
//!
//! 1. Read `garagetytus.toml` to extract the admin token + admin
//!    API URL.
//! 2. Call `GET /v1/health` to confirm the daemon is running.
//! 3. Call `GET /v1/cluster/layout` to read the current node UUID.
//! 4. Call `POST /v1/cluster/layout` to assign the node a tier
//!    (zone="local", capacity=1) — one-node cluster.
//! 5. Call `POST /v1/cluster/layout/apply?version=1` to commit.
//! 6. Provision an S3 service keypair via `garage key create
//!    s3-service`, store the access/secret pair in the OS
//!    keychain under (service="garagetytus", account="s3-service").
//! 7. Print a summary.
//!
//! Idempotent — re-running on a bootstrapped host detects the
//! existing layout + keypair and skips.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::context::CliContext;
use garagetytus_core::SecretsStore;

const SERVICE_KEY_LABEL: &str = "s3-service";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
/// AC8 — total wall-clock budget the post-spawn repair flow gets to
/// wait for `garage` to become healthy before giving up. Garage
/// usually binds within 1–2 s; 15 s leaves slack for slow disks.
const AUTO_REPAIR_HEALTH_BUDGET: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct GarageConfigToml {
    admin: AdminSection,
}

#[derive(Debug, Deserialize)]
struct AdminSection {
    api_bind_addr: String,
    admin_token: String,
}

pub async fn run(_ctx: &CliContext) -> Result<i32> {
    let cfg_path = garagetytus_core::paths::config_dir()
        .join("garagetytus.toml");
    if !cfg_path.exists() {
        eprintln!(
            "garagetytus bootstrap: config missing at {} — run `garagetytus install` first.",
            cfg_path.display()
        );
        return Ok(1);
    }
    let (admin_url, admin_token) = read_admin_credentials(&cfg_path)?;
    println!("garagetytus bootstrap: using admin API at {}", admin_url);

    if !health_ok(&admin_url, &admin_token).await? {
        eprintln!(
            "garagetytus bootstrap: daemon not responding at {} — \
             run `garagetytus start` first.",
            admin_url
        );
        return Ok(1);
    }
    println!("  daemon health: ok");

    assign_layout(&admin_url, &admin_token).await?;
    println!("  layout: assigned (zone=local, capacity=1)");

    if SecretsStore::get(SERVICE_KEY_LABEL).is_ok() {
        println!(
            "  service keypair: already in keychain (service=garagetytus, account={})",
            SERVICE_KEY_LABEL
        );
    } else {
        let (access, secret) = create_service_key(&cfg_path)?;
        store_creds(&access, &secret)?;
        println!(
            "  service keypair: created + stored in keychain (account={})",
            SERVICE_KEY_LABEL
        );
    }

    println!(
        "garagetytus bootstrap: done. Try `garagetytus bucket create demo --ttl 1h`."
    );
    Ok(0)
}

fn read_admin_credentials(cfg_path: &Path) -> Result<(String, String)> {
    let body = fs::read_to_string(cfg_path)
        .with_context(|| format!("read {}", cfg_path.display()))?;
    let parsed: GarageConfigToml = toml::from_str(&body)
        .context("parse garagetytus.toml — expected `[admin] api_bind_addr + admin_token`")?;
    let url = if parsed.admin.api_bind_addr.starts_with("http") {
        parsed.admin.api_bind_addr
    } else {
        format!("http://{}", parsed.admin.api_bind_addr)
    };
    Ok((url, parsed.admin.admin_token))
}

async fn health_ok(admin_url: &str, admin_token: &str) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()?;
    let url = format!("{}/v1/health", admin_url);
    match client
        .get(&url)
        .bearer_auth(admin_token)
        .send()
        .await
    {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(e) if e.is_connect() || e.is_timeout() => Ok(false),
        Err(e) => Err(anyhow!("admin health probe failed: {}", e)),
    }
}

/// v0.5 Tier-1 fix #1 — Garage 2.x moved the layout admin API
/// from `/v1/cluster/layout` to `/v2/{Get,Update,Apply}ClusterLayout`,
/// with body shapes that diverge from the v1 surface. Rather than
/// track the upstream HTTP wire format ourselves and risk drifting
/// again on the next major bump, shell out to the `garage` CLI —
/// upstream maintains the CLI in lockstep with the API. The CLI
/// uses the same config file we already wrote (`-c <cfg>`) so no
/// extra auth handling is needed (it reads `rpc_secret` directly,
/// not the admin token).
///
/// Layout assignment grammar (Garage 2.x):
///   garage layout assign <node-id> --capacity <bytes> --zone <z> --tag <t>
///   garage layout apply  --version <N>
///
/// Idempotent — `garage layout show` reports an empty `staged` +
/// non-empty `roles` after a successful assign+apply, which we
/// detect to skip on re-run.
fn assign_layout_via_cli(cfg_path: &Path) -> Result<()> {
    let garage_bin = locate_garage();

    // Probe current layout state via `garage status`. Output
    // includes the node ID + role assignment in tabular form.
    // Empty role column means "no layout yet"; once roles are
    // applied the column carries the zone/capacity tag.
    let status_out = Command::new(&garage_bin)
        .arg("-c")
        .arg(cfg_path)
        .arg("status")
        .output()
        .with_context(|| format!("spawn `{} status`", garage_bin.display()))?;
    if !status_out.status.success() {
        bail!(
            "garage status failed (exit {}): {}",
            status_out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&status_out.stderr)
        );
    }
    let status_text = String::from_utf8_lossy(&status_out.stdout).into_owned();
    let parsed = parse_garage_status(&status_text)?;
    if parsed.role_assigned {
        // Already bootstrapped — no work.
        return Ok(());
    }

    // Stage the layout: single-node, 100 GiB nominal capacity,
    // `local` zone+tag (matches the v0.1 single-node default;
    // `garagetytus cluster init` overrides for multi-node).
    let assign = Command::new(&garage_bin)
        .arg("-c")
        .arg(cfg_path)
        .args([
            "layout",
            "assign",
            &parsed.node_id,
            "--capacity",
            "100G",
            "--zone",
            "local",
            "--tag",
            "local",
        ])
        .output()
        .with_context(|| format!("spawn `{} layout assign`", garage_bin.display()))?;
    if !assign.status.success() {
        bail!(
            "garage layout assign failed (exit {}): {}",
            assign.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&assign.stderr)
        );
    }

    // Apply with `--version 1` for the first-ever apply on a
    // fresh cluster. If the layout was previously bumped (e.g.
    // a prior failed assign), re-running may need a different
    // version — fetch it from the staged state.
    let apply = Command::new(&garage_bin)
        .arg("-c")
        .arg(cfg_path)
        .args(["layout", "apply", "--version", "1"])
        .output()
        .with_context(|| format!("spawn `{} layout apply`", garage_bin.display()))?;
    if !apply.status.success() {
        // Tolerate "version mismatch" by re-reading the staged
        // version + retrying once; otherwise bail.
        let stderr = String::from_utf8_lossy(&apply.stderr);
        if let Some(ver) = parse_layout_apply_version_hint(&stderr) {
            let retry = Command::new(&garage_bin)
                .arg("-c")
                .arg(cfg_path)
                .args(["layout", "apply", "--version", &ver.to_string()])
                .output()?;
            if !retry.status.success() {
                bail!(
                    "garage layout apply failed (retry exit {}): {}",
                    retry.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&retry.stderr)
                );
            }
        } else {
            bail!(
                "garage layout apply failed (exit {}): {}",
                apply.status.code().unwrap_or(-1),
                stderr
            );
        }
    }

    Ok(())
}

/// Parse the relevant fields from `garage status` text output.
/// Looks for the local node's row in the HEALTHY NODES table
/// and extracts the node UUID + whether a role is assigned.
fn parse_garage_status(text: &str) -> Result<GarageStatus> {
    // The first table row that isn't a separator/header carries
    // the local node. Columns: ID Hostname Address Tags Zone
    // Capacity DataAvail Version. "NO ROLE ASSIGNED" appears
    // verbatim when the node has no layout role.
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("==") || trimmed.starts_with("ID ") {
            continue;
        }
        // First column is the 16-char node ID prefix.
        let id = trimmed.split_whitespace().next().unwrap_or_default();
        if id.len() < 8 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        return Ok(GarageStatus {
            node_id: id.to_string(),
            role_assigned: !trimmed.contains("NO ROLE ASSIGNED"),
        });
    }
    Err(anyhow!(
        "could not parse local node from `garage status` output:\n{}",
        text
    ))
}

#[derive(Debug, PartialEq, Eq)]
struct GarageStatus {
    node_id: String,
    role_assigned: bool,
}

/// Best-effort: extract a "use --version N" hint from the
/// upstream Garage error message when our initial apply was
/// rejected for a stale version number. Returns `None` if the
/// stderr doesn't carry a hint we recognize.
fn parse_layout_apply_version_hint(stderr: &str) -> Option<u64> {
    // Upstream message shape (paraphrased): "Cannot apply, expected
    // version N, got M". Pull the first number after "version".
    let lower = stderr.to_ascii_lowercase();
    let needle = "version ";
    let idx = lower.find(needle)?;
    let tail: String = lower[idx + needle.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    tail.parse().ok()
}

/// Async shim — keeps the existing call-site signature unchanged
/// while the implementation moved to a synchronous shellout. Called
/// once from `bootstrap::run`; not on a hot path.
async fn assign_layout(_admin_url: &str, _admin_token: &str) -> Result<()> {
    let cfg_path = garagetytus_core::paths::config_dir().join("garagetytus.toml");
    assign_layout_via_cli(&cfg_path)
}

fn create_service_key(cfg_path: &Path) -> Result<(String, String)> {
    let garage_bin = locate_garage();
    let out = Command::new(&garage_bin)
        .arg("-c")
        .arg(cfg_path)
        .args(["key", "create", SERVICE_KEY_LABEL])
        .output()
        .with_context(|| {
            format!(
                "spawning {} key create {}",
                garage_bin.display(),
                SERVICE_KEY_LABEL
            )
        })?;
    if !out.status.success() {
        bail!(
            "garage key create failed (exit {}): {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    parse_key_creds(&stdout)
}

fn locate_garage() -> std::path::PathBuf {
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

/// Parse `Key access ID: <access>` and `Secret access key: <secret>`
/// from `garage key create` output. Tolerant of the variant shapes
/// upstream emits (the same parser as `commands::bucket::parse_key_creds`).
fn parse_key_creds(out: &str) -> Result<(String, String)> {
    let mut access: Option<String> = None;
    let mut secret: Option<String> = None;
    for line in out.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("key id") || lower.contains("key access id") {
            if let Some(v) = trimmed.rsplit(':').next() {
                let v = v.trim().trim_matches('"').to_string();
                if !v.is_empty() && access.is_none() {
                    access = Some(v);
                }
            }
        } else if lower.contains("secret access key")
            || lower.starts_with("secret key:")
            || lower.contains("secret key:")
        {
            if let Some(v) = trimmed.rsplit(':').next() {
                let v = v.trim().trim_matches('"').to_string();
                if !v.is_empty() && secret.is_none() {
                    secret = Some(v);
                }
            }
        }
    }
    match (access, secret) {
        (Some(a), Some(s)) => Ok((a, s)),
        _ => Err(anyhow!(
            "could not parse access_key + secret_key from garage CLI output:\n---\n{out}\n---"
        )),
    }
}

fn store_creds(access: &str, secret: &str) -> Result<()> {
    let blob = json!({
        "access_key": access,
        "secret_key": secret,
        "endpoint":   "http://127.0.0.1:3900",
    })
    .to_string();
    SecretsStore::set(SERVICE_KEY_LABEL, &blob)
        .context("write s3-service creds to OS keychain")?;
    Ok(())
}

// ─── AC8 auto-repair (post-spawn, single-node only) ─────────

/// Outcome of `auto_repair_if_single_node`. Logged via tracing;
/// callers don't need to branch on this — it's purely diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairOutcome {
    /// Garage came up healthy + node count == 1 + `garage repair
    /// tables` exited 0. Counter `garagetytus_unclean_shutdown_total`
    /// stays incremented; the table-level integrity sweep ran.
    RepairRan,
    /// Garage came up healthy but the cluster has >1 node. Auto-repair
    /// was skipped because `repair tables` semantics differ across a
    /// network partition (Q3 verdict). Operator runs manually if
    /// needed. Carries the observed node count for logging.
    SkippedMultiNode { nodes: usize },
    /// Garage didn't pass the health probe within
    /// `AUTO_REPAIR_HEALTH_BUDGET`. Repair is best-effort, never
    /// blocks startup — this is a soft warning, not an error.
    HealthTimeout,
}

/// AC8 — call this AFTER spawning the garage subprocess, only when
/// `preflight_unclean_check` returned `Ok(true)`. Walks the admin API
/// to confirm: (1) garage is healthy, (2) we're on a single-node
/// cluster, then shells `garage repair tables` to nudge integrity.
///
/// Per Q3 verdict (LOPE pi+codex 2026-04-25): default-on for
/// single-node deployments (today's v0.1 reality), auto-skipped on
/// multi-node clusters. No flag, no operator ceremony. Failures of
/// any step are logged and swallowed — repair never blocks serve.
pub async fn auto_repair_if_single_node(cfg_path: &Path) -> Result<RepairOutcome> {
    let (admin_url, admin_token) = read_admin_credentials(cfg_path)?;

    if !wait_for_health(&admin_url, &admin_token, AUTO_REPAIR_HEALTH_BUDGET).await? {
        return Ok(RepairOutcome::HealthTimeout);
    }

    let nodes = probe_node_count(&admin_url, &admin_token).await?;
    if nodes != 1 {
        return Ok(RepairOutcome::SkippedMultiNode { nodes });
    }

    run_repair_tables(cfg_path)?;
    Ok(RepairOutcome::RepairRan)
}

/// Poll `health_ok` every 500 ms until it returns `Ok(true)` or
/// `deadline` passes. Returns `Ok(false)` on timeout.
async fn wait_for_health(
    admin_url: &str,
    admin_token: &str,
    budget: Duration,
) -> Result<bool> {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if health_ok(admin_url, admin_token).await? {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(false)
}

/// Read `GET /v1/cluster/layout` and return the number of nodes the
/// daemon knows about. v0.1 always returns 1 (single-node bootstrap);
/// the multi-node guard is belt-and-suspenders for v0.5+ topologies.
async fn probe_node_count(admin_url: &str, admin_token: &str) -> Result<usize> {
    let client = reqwest::Client::new();
    let layout: serde_json::Value = client
        .get(format!("{}/v1/cluster/layout", admin_url))
        .bearer_auth(admin_token)
        .send()
        .await
        .context("GET /v1/cluster/layout (probe_node_count)")?
        .error_for_status()?
        .json()
        .await
        .context("parse layout response (probe_node_count)")?;
    Ok(node_count_from_layout(&layout))
}

/// Pure function — extracted for unit testing on JSON fixtures.
/// Returns `0` for malformed payloads (treated as "skip repair" by
/// the caller, since the multi-node guard requires nodes == 1).
fn node_count_from_layout(layout: &serde_json::Value) -> usize {
    layout["nodes"]
        .as_array()
        .map(|arr| arr.len())
        .unwrap_or(0)
}

/// Shell `garage -c <cfg> repair tables --yes`. Runs against the
/// already-running daemon over its RPC channel. Idempotent + safe to
/// re-run. Sub-second on small clusters.
fn run_repair_tables(cfg_path: &Path) -> Result<()> {
    let garage_bin = locate_garage();
    let out = Command::new(&garage_bin)
        .arg("-c")
        .arg(cfg_path)
        .args(["repair", "tables", "--yes"])
        .output()
        .with_context(|| format!("spawning {} repair tables", garage_bin.display()))?;
    if !out.status.success() {
        bail!(
            "garage repair tables failed (exit {}): {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_creds_handles_canonical_shape() {
        let out = "Key access ID: GK1234567890abcdef\n\
                   Secret access key: \"secret-secret-secret\"\n";
        let (a, s) = parse_key_creds(out).unwrap();
        assert_eq!(a, "GK1234567890abcdef");
        assert_eq!(s, "secret-secret-secret");
    }

    #[test]
    fn parse_key_creds_handles_alt_shape() {
        let out = "Key ID: ABC123\nSecret key: XYZ789\n";
        let (a, s) = parse_key_creds(out).unwrap();
        assert_eq!(a, "ABC123");
        assert_eq!(s, "XYZ789");
    }

    #[test]
    fn parse_key_creds_errors_on_missing() {
        assert!(parse_key_creds("nothing here").is_err());
    }

    // ─── AC8 auto-repair tests ────────────────────────────

    #[test]
    fn node_count_from_layout_single_node() {
        let layout = serde_json::json!({
            "version": 1,
            "nodes": [{"id": "abc123", "addr": "127.0.0.1:3901"}],
            "roles": [{"id": "abc123", "zone": "local", "capacity": 1}]
        });
        assert_eq!(node_count_from_layout(&layout), 1);
    }

    #[test]
    fn node_count_from_layout_multi_node() {
        let layout = serde_json::json!({
            "version": 2,
            "nodes": [
                {"id": "n1", "addr": "10.0.0.1:3901"},
                {"id": "n2", "addr": "10.0.0.2:3901"},
                {"id": "n3", "addr": "10.0.0.3:3901"},
            ]
        });
        assert_eq!(node_count_from_layout(&layout), 3);
    }

    #[test]
    fn node_count_from_layout_empty_or_missing() {
        // Empty array → 0. Treated as "skip repair" (multi-node guard
        // gates on == 1, so anything != 1 means skip).
        let empty = serde_json::json!({"nodes": []});
        assert_eq!(node_count_from_layout(&empty), 0);

        // Missing field entirely → 0. Same behavior.
        let missing = serde_json::json!({"version": 1});
        assert_eq!(node_count_from_layout(&missing), 0);

        // Wrong type (object instead of array) → 0.
        let wrong_type = serde_json::json!({"nodes": {"a": 1}});
        assert_eq!(node_count_from_layout(&wrong_type), 0);
    }

    #[test]
    fn repair_outcome_variants_distinct() {
        // Sanity: the three outcomes the auto-repair flow can produce
        // are all distinguishable. Used by the tracing log line in
        // start.rs to differentiate "ran", "skipped multi-node", and
        // "health timed out".
        let a = RepairOutcome::RepairRan;
        let b = RepairOutcome::SkippedMultiNode { nodes: 3 };
        let c = RepairOutcome::HealthTimeout;
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert_eq!(b, RepairOutcome::SkippedMultiNode { nodes: 3 });
    }

    // ─── v0.5 Tier-1 fix #1 — garage CLI status parser tests ──

    #[test]
    fn parse_garage_status_empty_role() {
        // "NO ROLE ASSIGNED" — fresh single-node, before bootstrap.
        let out = "==== HEALTHY NODES ====\nID                Hostname  Address         Tags  Zone  Capacity          DataAvail  Version\n60b6ed95560189ac  ubuntu    127.0.0.1:3901              NO ROLE ASSIGNED             v2.3.0\n";
        let s = parse_garage_status(out).unwrap();
        assert_eq!(s.node_id, "60b6ed95560189ac");
        assert!(!s.role_assigned);
    }

    #[test]
    fn parse_garage_status_with_role() {
        // After `garage layout apply` — role + zone present.
        let out = "==== HEALTHY NODES ====\nID                Hostname  Address         Tags       Zone     Capacity  DataAvail          Version\n60b6ed95560189ac  ubuntu    127.0.0.1:3901  [droplet]  droplet  93.1 GiB  290.7 GiB (62.7%)  v2.3.0\n";
        let s = parse_garage_status(out).unwrap();
        assert_eq!(s.node_id, "60b6ed95560189ac");
        assert!(s.role_assigned);
    }

    #[test]
    fn parse_garage_status_skips_separators_and_headers() {
        let out = "\n\n==== HEALTHY NODES ====\nID                Hostname\n";
        // Pure header + separator — no node row → error.
        assert!(parse_garage_status(out).is_err());
    }

    #[test]
    fn parse_garage_status_rejects_non_hex_first_column() {
        let out = "==== HEALTHY NODES ====\nID                Hostname\nfoobar            ubuntu  127.0.0.1:3901  NO ROLE ASSIGNED\n";
        // "foobar" is not hex, gets skipped → no row found → error.
        assert!(parse_garage_status(out).is_err());
    }

    #[test]
    fn parse_layout_apply_version_hint_extracts_number() {
        // Synthetic upstream-shaped error message.
        let stderr = "Error: Cannot apply, expected version 3, got 1";
        assert_eq!(parse_layout_apply_version_hint(stderr), Some(3));
    }

    #[test]
    fn parse_layout_apply_version_hint_returns_none_on_unknown_shape() {
        assert_eq!(parse_layout_apply_version_hint("totally unrelated text"), None);
        assert_eq!(parse_layout_apply_version_hint(""), None);
    }

    #[test]
    fn read_admin_credentials_parses_seeded_config() {
        let body = r#"
metadata_dir = "/tmp/m"
data_dir = "/tmp/d"
db_engine = "sqlite"
replication_factor = 1
rpc_bind_addr = "127.0.0.1:3901"
rpc_public_addr = "127.0.0.1:3901"
rpc_secret = "dead"

[s3_api]
s3_region = "garage"
api_bind_addr = "127.0.0.1:3900"
root_domain = ".s3.garage.localhost"

[admin]
api_bind_addr = "127.0.0.1:3903"
admin_token   = "the-token"
metrics_token = "metrics-token"
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), body).unwrap();
        let (url, token) = read_admin_credentials(tmp.path()).unwrap();
        assert_eq!(url, "http://127.0.0.1:3903");
        assert_eq!(token, "the-token");
    }
}
