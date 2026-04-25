//! Clap subcommand surface.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Local S3 daemon for every dev laptop.",
    long_about = "garagetytus — installs, starts, and manages a local Garage \
S3 daemon on macOS or Linux. Bucket primitives + per-bucket \
sub-keypair grants are first-class. Windows targets v0.2."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
        /// One-time installer — downloads upstream Garage (Linux) or
    /// detects Homebrew-installed Garage (macOS), seeds config,
    /// registers the per-user service. On Windows, prints a
    /// v0.2-deferral notice and exits 0.
    Install,
    /// Uninstall — stop daemon, remove service unit + creds.
    /// Pass --keep-data to preserve bucket data on disk.
    Uninstall {
        /// Preserve bucket data on disk (config + creds + service
        /// unit are still removed).
        #[arg(long)]
        keep_data: bool,
    },
    /// Start the garagetytus service.
    Start,
    /// Stop the garagetytus service.
    Stop,
    /// Show service status (running / stopped, port bind).
    Status,
    /// Restart the garagetytus service.
    Restart,
    /// Run the daemon in the foreground (for users supplying their
    /// own supervisor — Docker, k8s, runit, manual launchctl).
    Serve,
    /// Bootstrap the running daemon: assign Garage layout, create
    /// the admin token, prepare the default endpoint.
    Bootstrap,
    /// Print version + bundled Garage version + AGPL upstream URL
    /// + path to THIRD_PARTY_NOTICES (Phase B.5).
    About,
    /// Bucket lifecycle.
    Bucket {
        #[command(subcommand)]
        cmd: BucketCmd,
    },
    /// v0.5 multinode cluster lifecycle (Mac + Tytus droplet).
    /// Per-subcommand details surface in the canonical sprint at
    /// `MAKAKOO/development/sprints/queued/MAKAKOO-OS-V0.8-S3-CLUSTER/`
    /// + the garagetytus-side wrapper at
    /// `GARAGETYTUS-V0.5-MULTINODE/`. Phase 0 probes gate Phase A;
    /// scaffolding lands ahead so Phase A integrates cleanly.
    Cluster {
        #[command(subcommand)]
        cmd: ClusterCmd,
    },
    /// Local-node Garage repair (`garage repair tables --yes`).
    /// In single-node mode this is invoked automatically by the
    /// AC8 unclean-shutdown flow (Q3 verdict, v0.1). In cluster
    /// mode, see `garagetytus cluster repair` for orchestrated
    /// per-node repair across the cluster.
    Repair,
}

/// `garagetytus cluster *` — Q4 verdict locked invocation
/// surface. Phase A.1 wires the SSH-driven `init` orchestration
/// once Phase 0 probes record their outcomes; the CLI shape
/// itself is final per the Q4-Q5-Q6 lope round 2026-04-25.
#[derive(Subcommand, Debug)]
pub enum ClusterCmd {
    /// Bootstrap a 2-node Garage cluster (Mac + droplet).
    /// Generates an `rpc_secret` if `--rpc-secret` is absent,
    /// writes `<config_dir>/cluster.toml`, and (in Phase A.1)
    /// SSHes into the droplet to push the binary + secret +
    /// systemd unit. The Q4 invocation host is garagetytus
    /// itself — tytus stays the env-var sidecar + WG tunnel
    /// owner, but does not own cluster orchestration.
    Init {
        /// Droplet SSH host (`user@host`). Required.
        #[arg(long)]
        droplet_host: String,

        /// 32-byte hex (64 chars). Generated if absent.
        #[arg(long)]
        rpc_secret: Option<String>,

        /// Mac zone name. Default `"mac"`. Immutable once set.
        #[arg(long)]
        mac_zone: Option<String>,

        /// Droplet zone name. Default `"droplet"`. Immutable.
        #[arg(long)]
        droplet_zone: Option<String>,

        /// Pod-facing endpoint over WG. Default
        /// `http://10.42.42.1:3900/`.
        #[arg(long)]
        pod_endpoint: Option<String>,

        /// Print the plan but make no changes.
        #[arg(long)]
        dry_run: bool,

        /// Re-run even if `cluster.toml` already exists.
        #[arg(long)]
        force: bool,
    },
    /// Print cluster status (per-zone mode + reachability +
    /// layout version + strict cluster_mode rollup). Reads
    /// `<config_dir>/cluster.toml` + `<data_dir>/cluster_state.json`.
    Status {
        /// Emit JSON envelope instead of human table.
        #[arg(long)]
        json: bool,
    },
    /// Orchestrate `garage repair tables --yes` across cluster
    /// nodes. Per Q5 verdict — local repair on each node,
    /// cluster anti-entropy reconciles cross-node drift.
    Repair {
        /// Comma-separated node list. Default: all (`mac,droplet`).
        #[arg(long, value_delimiter = ',')]
        nodes: Option<Vec<String>>,

        /// Skip the orphan-PID sentinel check; force repair.
        #[arg(long)]
        force: bool,

        /// Print the plan but make no changes.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Bucket subcommand surface, carved verbatim from
/// `makakoo-os/makakoo/src/cli.rs:532` (Phase A.2). Help text
/// + flags retained byte-for-byte; only the brand-name strings
/// change.
#[derive(Subcommand, Debug)]
pub enum BucketCmd {
    /// Create a new bucket on the chosen backend (default: local Garage).
    /// Default TTL is 7 days; default quota is 10 GB. Pass
    /// `--ttl permanent` or `--quota unlimited` with `--confirm-yes-really`
    /// to override.
    Create {
        /// Bucket name. 3–63 chars, lowercase letters / digits / dot /
        /// hyphen only; must start + end with alphanumeric. No
        /// underscores. Validated BEFORE backend dispatch.
        name: String,

        /// Backend endpoint name (defaults to local Garage).
        #[arg(long)]
        endpoint: Option<String>,

        /// TTL — `30m | 1h | 24h | 7d | permanent`. Default `7d`.
        #[arg(long, default_value = "7d")]
        ttl: String,

        /// Hard quota — e.g. `100M`, `1G`, `10G`, or `unlimited`.
        #[arg(long, default_value = "10G")]
        quota: String,

        /// Required to use `--ttl permanent` or `--quota unlimited`.
        #[arg(long)]
        confirm_yes_really: bool,
    },

    /// List buckets known to garagetytus on the chosen backend.
    List {
        /// Backend endpoint name (default: every registered endpoint).
        #[arg(long)]
        endpoint: Option<String>,
        /// Emit JSON instead of the default table.
        #[arg(long)]
        json: bool,
    },

    /// Show one bucket's metadata (TTL, quota, usage %, grants).
    Info {
        /// Bucket name.
        name: String,
        /// Emit JSON instead of the default human view.
        #[arg(long)]
        json: bool,
    },

    /// Grant a per-bucket scoped sub-keypair to a labeled consumer.
    /// Returns `(endpoint_url, access_key, secret_key, expires_at)`
    /// on stdout; the caller wires these into their own boto3 /
    /// aws-cli / rclone config.
    Grant {
        /// Bucket name.
        bucket: String,
        /// Human-readable label for the grantee — appears in
        /// audit log.
        #[arg(long)]
        to: String,
        /// Comma-separated permission set: `read`, `read,write`, or
        /// `read,write,owner`.
        #[arg(long, default_value = "read,write")]
        perms: String,
        /// TTL — `30m | 1h | 24h | 7d | permanent`. Default `1h`.
        #[arg(long, default_value = "1h")]
        ttl: String,
        /// Required to use `--ttl permanent`.
        #[arg(long)]
        confirm_yes_really: bool,
        /// Emit JSON instead of the default human view.
        #[arg(long)]
        json: bool,
    },

    /// Revoke a bucket grant by its ID. Atomic 3-state transition:
    /// `active → revoking → revoked`.
    Revoke {
        /// Grant ID (as printed by `bucket grant`).
        grant_id: String,
    },

    /// Walk the bucket registry and purge TTL'd buckets and TTL'd
    /// grants.
    Expire {
        /// Don't actually delete anything — just print what would happen.
        #[arg(long)]
        dry_run: bool,
    },

    /// Emergency stop: flip a bucket flag that makes Garage 403
    /// every read/write, including those carrying a still-valid
    /// presigned URL.
    DenyAll {
        /// Bucket name.
        name: String,
        /// TTL — flag clears automatically after this duration.
        /// Default `1h`. `--ttl permanent` requires
        /// `--confirm-yes-really`.
        #[arg(long, default_value = "1h")]
        ttl: String,
        /// Required to use `--ttl permanent`.
        #[arg(long)]
        confirm_yes_really: bool,
    },
}
