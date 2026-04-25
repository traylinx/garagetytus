//! `garagetytus install` — Phase B.1 cross-platform installer.
//!
//! - **macOS**: detect brew + `garage` on PATH (LD#7 amended —
//!   Homebrew compile-from-source path); seed
//!   `garagetytus.toml`; emit launchd plist via hand-rolled
//!   template (LD#3 fallback after `service-manager`'s plist
//!   builder failed the P0.2 quality gate); `launchctl load`.
//! - **Linux**: download upstream Garage musl binary from
//!   `garagehq.deuxfleurs.fr/_releases/v2.3.0/<target>/garage`;
//!   SHA-verify against the per-target pin in `versions.toml`;
//!   install to `~/.local/bin/garage`; seed config; emit
//!   systemd-user unit; `systemctl --user daemon-reload`.
//! - **Windows**: deferred to v0.2 per Q1 verdict; prints the
//!   notice + exits 0.
//!
//! Idempotent — re-running on a populated install is a no-op.
//! AC2 / AC5 / AC6 / AC7 acceptance.

// Per-OS cfg gating naturally creates `dead_code` warnings on the
// non-active branches (Linux SHAs unused on Mac etc). The whole
// module is correctness-gated by tests; suppress the noise.
#![allow(dead_code)]

use std::fs;
use std::path::Path;

#[allow(unused_imports)]
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use rand::RngCore;

use crate::context::CliContext;

const WINDOWS_DEFERRAL: &str =
    "v0.1 ships Mac + Linux only. Windows support targets v0.2. \
See README §Windows.";

/// Pinned upstream Garage version.
pub const GARAGE_VERSION: &str = "v2.3.0";

/// SHA-256 pin per upstream Linux target (from `versions.toml`).
/// AGPL acquisition contract: refusing to install on
/// SHA-mismatch is the AC7 hard-fail.
const LINUX_X86_64_SHA: &str =
    "f98d317942bb341151a2775162016bb50cf86b865d0108de03eb5db16e2120cd";
const LINUX_AARCH64_SHA: &str =
    "8ced2ad3040262571de08aa600959aa51f97576d55da7946fcde6f66140705e2";

const PLIST_LABEL: &str = "com.traylinx.garagetytus";
const SERVICE_UNIT_NAME: &str = "garagetytus.service";

pub async fn run(ctx: &CliContext) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        let _ = ctx;
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    #[cfg(target_os = "macos")]
    {
        return mac_install(ctx).await;
    }

    #[cfg(target_os = "linux")]
    {
        return linux_install(ctx).await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = ctx;
        bail!(
            "unsupported target — garagetytus v0.1 supports macOS, Linux, and Windows-deferred (got {})",
            std::env::consts::OS
        );
    }
}

// ─── macOS ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn mac_install(ctx: &CliContext) -> Result<i32> {
    use std::process::Command;
    println!("garagetytus install: macOS path");

    if Command::new("which").arg("brew").output()?.status.success() == false {
        eprintln!("garagetytus install: Homebrew not found.");
        eprintln!("  Install Homebrew first: https://brew.sh");
        return Ok(1);
    }
    let garage_which = Command::new("which").arg("garage").output()?;
    if !garage_which.status.success() {
        eprintln!("garagetytus install: `garage` not on PATH.");
        eprintln!("  Run: brew install garage");
        eprintln!(
            "  Or:  brew install traylinx/tap/garagetytus (also pulls Garage)"
        );
        return Ok(1);
    }
    let garage_bin = String::from_utf8_lossy(&garage_which.stdout)
        .trim()
        .to_string();
    println!("  garage binary at: {}", garage_bin);

    let config_dir = garagetytus_core::paths::config_dir();
    let data_dir = garagetytus_core::paths::data_dir();
    let log_dir = garagetytus_core::paths::log_dir();
    fs::create_dir_all(&config_dir).context("create config dir")?;
    fs::create_dir_all(&data_dir).context("create data dir")?;
    fs::create_dir_all(&log_dir).context("create log dir")?;
    fs::create_dir_all(data_dir.join("garage")).context("create garage data dir")?;
    fs::create_dir_all(data_dir.join("garage-meta")).context("create garage meta dir")?;

    let cfg_path = config_dir.join("garagetytus.toml");
    write_garage_config_if_missing(&cfg_path, &data_dir)?;

    let plist_dir =
        dirs::home_dir().unwrap_or_default().join("Library/LaunchAgents");
    fs::create_dir_all(&plist_dir).context("create LaunchAgents dir")?;
    let plist_path = plist_dir.join(format!("{}.plist", PLIST_LABEL));
    write_plist(
        &plist_path,
        &PlistInputs {
            label: PLIST_LABEL,
            garage_bin: &garage_bin,
            config_path: &cfg_path,
            log_dir: &log_dir,
        },
    )?;
    println!("  plist seeded at {}", plist_path.display());

    println!(
        "garagetytus install: done. Next: garagetytus start && garagetytus bootstrap"
    );
    let _ = ctx;
    Ok(0)
}

// ─── Linux ───────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn linux_install(ctx: &CliContext) -> Result<i32> {
    println!("garagetytus install: Linux path");

    let target = match std::env::consts::ARCH {
        "x86_64" => ("x86_64-unknown-linux-musl", LINUX_X86_64_SHA),
        "aarch64" => ("aarch64-unknown-linux-musl", LINUX_AARCH64_SHA),
        other => bail!(
            "unsupported Linux arch: {} — v0.1 supports x86_64 + aarch64 musl",
            other
        ),
    };
    let (target_triple, expected_sha) = target;
    println!(
        "  target: {} (SHA-256 {} pinned in versions.toml)",
        target_triple, expected_sha
    );

    let bin_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".local/bin");
    fs::create_dir_all(&bin_dir).context("create ~/.local/bin")?;
    let garage_bin = bin_dir.join("garage");

    if !garage_bin.exists() {
        let url = format!(
            "https://garagehq.deuxfleurs.fr/_releases/{}/{}/garage",
            GARAGE_VERSION, target_triple
        );
        println!("  downloading {}", url);
        download_and_verify(&url, &garage_bin, expected_sha).await?;
        println!("  installed {}", garage_bin.display());
    } else {
        println!("  garage binary already present at {}", garage_bin.display());
        verify_sha(&garage_bin, expected_sha).context(
            "garage binary on disk does not match pinned SHA — \
             run `rm` then re-install (AC7)",
        )?;
    }

    let config_dir = garagetytus_core::paths::config_dir();
    let data_dir = garagetytus_core::paths::data_dir();
    let log_dir = garagetytus_core::paths::log_dir();
    fs::create_dir_all(&config_dir).context("create config dir")?;
    fs::create_dir_all(&data_dir).context("create data dir")?;
    fs::create_dir_all(&log_dir).context("create log dir")?;
    fs::create_dir_all(data_dir.join("garage")).context("create garage data dir")?;
    fs::create_dir_all(data_dir.join("garage-meta")).context("create garage meta dir")?;

    let cfg_path = config_dir.join("garagetytus.toml");
    write_garage_config_if_missing(&cfg_path, &data_dir)?;

    let unit_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("systemd/user");
    fs::create_dir_all(&unit_dir).context("create systemd user unit dir")?;
    let unit_path = unit_dir.join(SERVICE_UNIT_NAME);
    write_systemd_unit(
        &unit_path,
        &SystemdInputs {
            garage_bin: garage_bin.to_str().unwrap_or("garage"),
            config_path: cfg_path.to_str().unwrap_or(""),
            log_dir: log_dir.to_str().unwrap_or(""),
        },
    )?;
    println!("  systemd unit seeded at {}", unit_path.display());

    println!(
        "garagetytus install: done. Next: garagetytus start && garagetytus bootstrap"
    );
    let _ = ctx;
    Ok(0)
}

// ─── uninstall ───────────────────────────────────────────────

pub async fn uninstall(_ctx: &CliContext, keep_data: bool) -> Result<i32> {
    #[cfg(target_os = "windows")]
    {
        let _ = (_ctx, keep_data);
        eprintln!("{}", WINDOWS_DEFERRAL);
        return Ok(0);
    }

    println!(
        "garagetytus uninstall: removing service + config{}",
        if keep_data {
            " (data preserved via --keep-data)"
        } else {
            " + data"
        }
    );

    // 1. Best-effort stop. Failures are fine — we may already be
    //    stopped, or the binary may have crashed.
    let _ = crate::commands::start::stop(_ctx);

    // 2. Remove service unit (per-OS).
    #[cfg(target_os = "macos")]
    {
        let plist = dirs::home_dir()
            .unwrap_or_default()
            .join("Library/LaunchAgents")
            .join("com.traylinx.garagetytus.plist");
        remove_if_exists(&plist, "plist");
    }
    #[cfg(target_os = "linux")]
    {
        let unit = dirs::config_dir()
            .unwrap_or_default()
            .join("systemd/user/garagetytus.service");
        remove_if_exists(&unit, "systemd unit");
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }

    // 3. Remove the s3-service keychain entry. Idempotent —
    //    SecretsStore::delete swallows NoEntry.
    if let Err(e) = garagetytus_core::SecretsStore::delete("s3-service") {
        eprintln!(
            "  warning: could not remove s3-service keychain entry: {} (skipping)",
            e
        );
    } else {
        println!("  keychain: s3-service removed");
    }

    // 4. Remove config + (optionally) data. Honors GARAGETYTUS_HOME.
    let config_dir = garagetytus_core::paths::config_dir();
    let log_dir = garagetytus_core::paths::log_dir();
    remove_dir_if_exists(&config_dir, "config");
    remove_dir_if_exists(&log_dir, "logs");
    if !keep_data {
        let data_dir = garagetytus_core::paths::data_dir();
        remove_dir_if_exists(&data_dir, "data");
    }

    println!("garagetytus uninstall: done");
    Ok(0)
}

fn remove_if_exists(path: &Path, label: &str) {
    if !path.exists() {
        return;
    }
    match fs::remove_file(path) {
        Ok(()) => println!("  {}: removed {}", label, path.display()),
        Err(e) => eprintln!(
            "  warning: could not remove {} {}: {} (skipping)",
            label,
            path.display(),
            e
        ),
    }
}

fn remove_dir_if_exists(path: &Path, label: &str) {
    if !path.exists() {
        return;
    }
    match fs::remove_dir_all(path) {
        Ok(()) => println!("  {}: removed {}", label, path.display()),
        Err(e) => eprintln!(
            "  warning: could not remove {} {}: {} (skipping)",
            label,
            path.display(),
            e
        ),
    }
}

// ─── shared helpers ──────────────────────────────────────────

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

fn random_url_safe_b64(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buf)
}

fn write_garage_config_if_missing(path: &Path, data_dir: &Path) -> Result<()> {
    if path.exists() {
        println!("  config already present at {}", path.display());
        return Ok(());
    }
    let body = render_garage_config(data_dir);
    fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
    set_private_mode(path);
    println!("  config seeded at {}", path.display());
    Ok(())
}

pub(crate) fn render_garage_config(data_dir: &Path) -> String {
    let rpc_secret = random_hex(32);
    let admin_token = random_url_safe_b64(32);
    let metrics_token = random_url_safe_b64(32);
    let data = data_dir.join("garage").display().to_string();
    let meta = data_dir.join("garage-meta").display().to_string();
    format!(
        r#"# garagetytus.toml — generated by `garagetytus install`.
# Tokens are random per-host; rotate by running `garagetytus install
# --rotate-tokens` (Phase B.4).

metadata_dir = "{meta}"
data_dir     = "{data}"
db_engine    = "sqlite"
replication_factor = 1
rpc_bind_addr = "127.0.0.1:3901"
rpc_public_addr = "127.0.0.1:3901"
rpc_secret    = "{rpc_secret}"

[s3_api]
s3_region    = "garage"
api_bind_addr = "127.0.0.1:3900"
root_domain   = ".s3.garage.localhost"

[admin]
api_bind_addr = "127.0.0.1:3903"
admin_token   = "{admin_token}"
metrics_token = "{metrics_token}"
"#
    )
}

fn set_private_mode(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(path, perms);
        }
    }
    let _ = path;
}

// ─── plist renderer (macOS, hand-rolled per LD#3 fallback) ──

#[cfg(target_os = "macos")]
struct PlistInputs<'a> {
    label: &'a str,
    garage_bin: &'a str,
    config_path: &'a Path,
    log_dir: &'a Path,
}

#[cfg(target_os = "macos")]
fn write_plist(path: &Path, inputs: &PlistInputs) -> Result<()> {
    let body = render_plist(inputs);
    fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn render_plist(inputs: &PlistInputs) -> String {
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    let log = inputs.log_dir.display();
    let cfg = inputs.config_path.display();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
      <string>{garage}</string>
      <string>-c</string>
      <string>{cfg}</string>
      <string>server</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}/garage.out.log</string>
    <key>StandardErrorPath</key>
    <string>{log}/garage.err.log</string>
    <key>EnvironmentVariables</key>
    <dict>
      <key>HOME</key>
      <string>{home}</string>
    </dict>
    <key>ProcessType</key>
    <string>Background</string>
  </dict>
</plist>
"#,
        label = inputs.label,
        garage = inputs.garage_bin,
    )
}

// ─── systemd-user unit renderer (Linux) ──────────────────────

#[cfg(target_os = "linux")]
struct SystemdInputs<'a> {
    garage_bin: &'a str,
    config_path: &'a str,
    log_dir: &'a str,
}

#[cfg(target_os = "linux")]
fn write_systemd_unit(path: &Path, inputs: &SystemdInputs) -> Result<()> {
    let body = render_systemd_unit(inputs);
    fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn render_systemd_unit(inputs: &SystemdInputs) -> String {
    format!(
        r#"[Unit]
Description=garagetytus — local Garage S3 daemon (LD#7 musl binary)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={garage} -c {cfg} server
Restart=always
RestartSec=5
StandardOutput=append:{log}/garage.out.log
StandardError=append:{log}/garage.err.log

[Install]
WantedBy=default.target
"#,
        garage = inputs.garage_bin,
        cfg = inputs.config_path,
        log = inputs.log_dir,
    )
}

// ─── Linux download + SHA verify ─────────────────────────────

#[cfg(target_os = "linux")]
async fn download_and_verify(
    url: &str,
    dest: &Path,
    expected_sha: &str,
) -> Result<()> {
    let bytes = reqwest::get(url)
        .await
        .with_context(|| format!("GET {}", url))?
        .error_for_status()
        .with_context(|| format!("non-2xx for {}", url))?
        .bytes()
        .await
        .context("read response body")?;
    let actual = sha256_hex(&bytes);
    if actual != expected_sha {
        bail!(
            "SHA mismatch (AC7 hard-fail). expected {} actual {} url {}",
            expected_sha,
            actual,
            url
        );
    }
    fs::write(dest, &bytes).with_context(|| format!("write {}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dest, perms)?;
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// AC7 verifier — re-reads `path` from disk and compares its
/// SHA-256 against the pinned `expected_sha`. Surfaces the
/// concrete (expected, actual) hex pair on mismatch so tampering
/// is auditable. Cross-platform — used on Linux for boot-time
/// pin enforcement, available on every target for test harnesses.
fn verify_sha(path: &Path, expected_sha: &str) -> Result<()> {
    let bytes =
        fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let actual = sha256_hex(&bytes);
    if actual != expected_sha {
        return Err(anyhow!(
            "SHA mismatch on {}: expected {} actual {}",
            path.display(),
            expected_sha,
            actual
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn random_hex_correct_length() {
        let s = random_hex(32);
        assert_eq!(s.len(), 64); // 32 bytes → 64 hex chars
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn random_url_safe_b64_no_padding() {
        let s = random_url_safe_b64(32);
        assert!(!s.contains('='));
        assert!(!s.contains('+'));
        assert!(!s.contains('/'));
        assert!(s.len() >= 40);
    }

    #[test]
    fn render_garage_config_is_valid_toml() {
        let tmp = tempdir().unwrap();
        let body = render_garage_config(tmp.path());
        let parsed: toml::Value = toml::from_str(&body).expect("valid toml");
        assert_eq!(parsed["db_engine"].as_str(), Some("sqlite"));
        assert_eq!(parsed["replication_factor"].as_integer(), Some(1));
        assert_eq!(
            parsed["s3_api"]["api_bind_addr"].as_str(),
            Some("127.0.0.1:3900")
        );
        assert_eq!(
            parsed["admin"]["api_bind_addr"].as_str(),
            Some("127.0.0.1:3903")
        );
    }

    #[test]
    fn render_garage_config_tokens_unique() {
        let tmp = tempdir().unwrap();
        let a = render_garage_config(tmp.path());
        let b = render_garage_config(tmp.path());
        assert_ne!(a, b, "tokens should be random per render");
    }

    #[test]
    fn write_garage_config_if_missing_skips_existing() {
        let tmp = tempdir().unwrap();
        let cfg = tmp.path().join("g.toml");
        fs::write(&cfg, "untouched").unwrap();
        write_garage_config_if_missing(&cfg, tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&cfg).unwrap(), "untouched");
    }

    #[test]
    fn write_garage_config_if_missing_seeds_new() {
        let tmp = tempdir().unwrap();
        let cfg = tmp.path().join("g.toml");
        write_garage_config_if_missing(&cfg, tmp.path()).unwrap();
        assert!(cfg.exists());
        let body = fs::read_to_string(&cfg).unwrap();
        assert!(body.contains("rpc_secret"));
        assert!(body.contains("admin_token"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn render_plist_carries_required_fields() {
        // P0.2 quality gate: KeepAlive, RunAtLoad, StandardOutPath,
        // StandardErrorPath, ProcessType=Background.
        let inputs = PlistInputs {
            label: "com.test.example",
            garage_bin: "/usr/local/bin/garage",
            config_path: Path::new("/tmp/g.toml"),
            log_dir: Path::new("/tmp/logs"),
        };
        let xml = render_plist(&inputs);
        for needle in [
            "<key>KeepAlive</key>",
            "<key>RunAtLoad</key>",
            "<key>StandardOutPath</key>",
            "<key>StandardErrorPath</key>",
            "<key>ProcessType</key>",
            "<string>Background</string>",
            "<string>com.test.example</string>",
            "/usr/local/bin/garage",
        ] {
            assert!(xml.contains(needle), "plist missing {}", needle);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn render_systemd_unit_carries_required_fields() {
        let inputs = SystemdInputs {
            garage_bin: "/home/x/.local/bin/garage",
            config_path: "/home/x/.config/garagetytus/garagetytus.toml",
            log_dir: "/home/x/.local/share/garagetytus/logs",
        };
        let unit = render_systemd_unit(&inputs);
        for needle in [
            "[Unit]",
            "[Service]",
            "Type=simple",
            "Restart=always",
            "WantedBy=default.target",
            "/home/x/.local/bin/garage",
        ] {
            assert!(unit.contains(needle), "unit missing {}", needle);
        }
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // Test vector: SHA-256("abc") =
        // ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// AC7 — tampered binary fails SHA verify with a clear
    /// (expected, actual) message. Writes "abc" to a temp file
    /// (known SHA) then asserts the wrong-SHA path errors.
    #[test]
    fn verify_sha_rejects_tampered_binary() {
        let tmp = tempdir().unwrap();
        let bin = tmp.path().join("garage");
        std::fs::write(&bin, b"abc").unwrap();

        // Right SHA: Ok.
        let right_sha =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        verify_sha(&bin, right_sha).expect("matching SHA should pass");

        // Wrong SHA: Err with the diff surfaced.
        let wrong_sha =
            "0000000000000000000000000000000000000000000000000000000000000000";
        let err = verify_sha(&bin, wrong_sha).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("SHA mismatch"),
            "expected SHA mismatch, got: {}",
            msg
        );
        assert!(
            msg.contains(right_sha),
            "expected message to surface actual SHA: {}",
            msg
        );
        assert!(
            msg.contains(wrong_sha),
            "expected message to surface expected SHA: {}",
            msg
        );
    }

    /// AC7 — verify_sha errors cleanly when the file doesn't
    /// exist (rather than silently passing).
    #[test]
    fn verify_sha_errors_on_missing_file() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("nope");
        let err = verify_sha(&missing, "abc").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("read"), "unexpected error: {}", msg);
    }
}
