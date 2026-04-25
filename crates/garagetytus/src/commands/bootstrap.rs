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
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::context::CliContext;
use garagetytus_core::SecretsStore;

const SERVICE_KEY_LABEL: &str = "s3-service";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

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

async fn assign_layout(admin_url: &str, admin_token: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let layout: serde_json::Value = client
        .get(format!("{}/v1/cluster/layout", admin_url))
        .bearer_auth(admin_token)
        .send()
        .await
        .context("GET /v1/cluster/layout")?
        .error_for_status()?
        .json()
        .await
        .context("parse layout response")?;

    // Already assigned?
    if let Some(roles) = layout["roles"].as_array() {
        if !roles.is_empty() {
            return Ok(());
        }
    }
    // Discover the node UUID — Garage exposes it under
    // `nodes` with `addr`/`id` fields. Fallback: `garage status`.
    let node_id = layout["nodes"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|n| n["id"].as_str())
        .map(String::from)
        .ok_or_else(|| {
            anyhow!(
                "could not parse node id from layout response: {}",
                layout
            )
        })?;

    let stage = json!([
        {
            "id": node_id,
            "zone": "local",
            "capacity": 1_073_741_824u64, // 1 GiB nominal — single-node
            "tags": ["local"]
        }
    ]);
    client
        .post(format!("{}/v1/cluster/layout", admin_url))
        .bearer_auth(admin_token)
        .json(&stage)
        .send()
        .await
        .context("POST /v1/cluster/layout")?
        .error_for_status()?;

    let next_version = layout["version"].as_u64().unwrap_or(0) + 1;
    client
        .post(format!(
            "{}/v1/cluster/layout/apply?version={}",
            admin_url, next_version
        ))
        .bearer_auth(admin_token)
        .send()
        .await
        .context("POST /v1/cluster/layout/apply")?
        .error_for_status()?;

    Ok(())
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
