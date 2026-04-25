//! `cluster` — type system for v0.5 multinode Garage cluster
//! support. Pure types + path resolution + serde; no network,
//! no SSH, no subprocess execution.
//!
//! v0.5 ships a 2-node cluster (Mac + Tytus control-plane droplet)
//! per the canonical
//! `MAKAKOO-OS-V0.8-S3-CLUSTER/SPRINT.md` spec, hardened by
//! pi+qwen lope round 1, with three additional garagetytus-side
//! design questions (Q4 init host, Q5 auto-repair, Q6 mode
//! derivation) locked at
//! `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/verdicts/`.
//!
//! Phase 0 probe outcomes gate Phase A; this module ships the
//! Phase-0-independent type surface so Phase A can land cleanly
//! the moment Phase 0 results are recorded.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Cluster configuration TOML, written by `garagetytus cluster
/// init` and read by every cluster-aware command. Lives at
/// `<config_dir>/cluster.toml` (LD#9 path resolution).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Schema version. v0.5 ships `1`. Bumped when the file
    /// format changes incompatibly; the cluster CLI refuses to
    /// load a config with an unknown version.
    pub schema_version: u32,
    /// 32-byte hex (64 chars). Generated via `openssl rand -hex
    /// 32` if absent at `init` time. Stored cleartext on this
    /// host (Mac keychain mirror lives at
    /// `(garagetytus, cluster-rpc-secret)`); the droplet copy
    /// lives at `/etc/garagetytus/cluster_rpc_secret` mode 0600.
    /// Distribution over SSH (encrypted), NEVER over the WG
    /// tunnel (LD#5 of canonical spec).
    pub rpc_secret: String,
    /// Local Mac zone name. Default `"mac"`. Immutable once set
    /// (renaming requires full cluster reconfig — landmine in
    /// canonical spec).
    pub mac_zone: String,
    /// Droplet zone name. Default `"droplet"`. Immutable once set.
    pub droplet_zone: String,
    /// SSH host string (`user@host`) for the droplet. Read by
    /// `cluster init` + `cluster repair` to orchestrate
    /// SSH-driven steps.
    pub droplet_host: String,
    /// Pod-facing endpoint that pods route to over WG. Default
    /// `http://10.42.42.1:3900/` (the droplet's WG-bound S3 API).
    /// Q4 verdict added this as an override-friendly knob for
    /// non-Sebastian deployments.
    pub pod_endpoint: String,
    /// Replication factor — number of object copies. v0.5 fixes
    /// at `2` (one per zone); higher counts queued for v0.9
    /// when N>2 nodes are supported.
    pub replication_factor: u8,
}

impl ClusterConfig {
    /// Current schema version. Bump only when the file format
    /// changes incompatibly.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Default mac zone name. Locked immutable per LD#9 of the
    /// canonical sprint.
    pub const DEFAULT_MAC_ZONE: &'static str = "mac";

    /// Default droplet zone name. Locked immutable per LD#9 of
    /// the canonical sprint.
    pub const DEFAULT_DROPLET_ZONE: &'static str = "droplet";

    /// Default pod-facing endpoint. The WG IP `10.42.42.1` is
    /// constant per the Tytus tunnel design; the port matches
    /// Garage's S3 API on `0.0.0.0:3900` (iptables-restricted to
    /// `10.42.42.0/24` per LD#4 of the canonical sprint).
    pub const DEFAULT_POD_ENDPOINT: &'static str = "http://10.42.42.1:3900/";

    /// Construct a fresh config from the raw inputs. Generates
    /// no rpc_secret — caller produces that via
    /// `openssl rand -hex 32` (or equivalent crypto-RNG) before
    /// calling.
    pub fn new(
        rpc_secret: String,
        droplet_host: String,
        mac_zone: Option<String>,
        droplet_zone: Option<String>,
        pod_endpoint: Option<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            rpc_secret,
            mac_zone: mac_zone
                .unwrap_or_else(|| Self::DEFAULT_MAC_ZONE.to_string()),
            droplet_zone: droplet_zone
                .unwrap_or_else(|| Self::DEFAULT_DROPLET_ZONE.to_string()),
            droplet_host,
            pod_endpoint: pod_endpoint
                .unwrap_or_else(|| Self::DEFAULT_POD_ENDPOINT.to_string()),
            replication_factor: 2,
        }
    }

    /// Validate basic invariants (rpc_secret length, zone-name
    /// non-empty, replication_factor in [1, 8]). Doesn't probe
    /// the network or SSH host.
    pub fn validate(&self) -> Result<(), ClusterConfigError> {
        if self.schema_version != Self::SCHEMA_VERSION {
            return Err(ClusterConfigError::SchemaMismatch {
                got: self.schema_version,
                want: Self::SCHEMA_VERSION,
            });
        }
        if self.rpc_secret.len() != 64 {
            return Err(ClusterConfigError::InvalidSecret {
                len: self.rpc_secret.len(),
            });
        }
        if !self
            .rpc_secret
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        {
            return Err(ClusterConfigError::InvalidSecret {
                len: self.rpc_secret.len(),
            });
        }
        if self.mac_zone.is_empty() || self.droplet_zone.is_empty() {
            return Err(ClusterConfigError::EmptyZone);
        }
        if self.mac_zone == self.droplet_zone {
            return Err(ClusterConfigError::ZoneCollision {
                zone: self.mac_zone.clone(),
            });
        }
        if self.droplet_host.is_empty() {
            return Err(ClusterConfigError::EmptyDropletHost);
        }
        if !(1..=8).contains(&self.replication_factor) {
            return Err(ClusterConfigError::InvalidReplicationFactor {
                got: self.replication_factor,
            });
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClusterConfigError {
    #[error("cluster.toml schema version {got} does not match expected {want}")]
    SchemaMismatch { got: u32, want: u32 },
    #[error("rpc_secret must be 64 hex chars; got len={len}")]
    InvalidSecret { len: usize },
    #[error("mac_zone and droplet_zone must be non-empty")]
    EmptyZone,
    #[error("mac_zone and droplet_zone must differ; both = {zone}")]
    ZoneCollision { zone: String },
    #[error("droplet_host must be non-empty")]
    EmptyDropletHost,
    #[error("replication_factor must be in [1, 8]; got {got}")]
    InvalidReplicationFactor { got: u8 },
}

/// Cluster runtime state, written by Garage's admin API and
/// mirrored by the watchdog tick. Lives at
/// `<data_dir>/cluster_state.json`. Read by `cluster status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterState {
    /// Schema version. v0.5 ships `1`.
    pub schema_version: u32,
    /// Per-node liveness as last observed by the local watchdog.
    /// Keyed by zone name.
    pub nodes: BTreeMap<String, NodeState>,
    /// Garage layout version observed locally. Bumps every time
    /// `garage layout apply` succeeds.
    pub layout_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeState {
    /// `true` if the node responded to the last RPC heartbeat.
    pub reachable: bool,
    /// Wallclock seconds since the last successful heartbeat.
    /// `None` if never observed since process start.
    pub last_heartbeat_unix_seconds: Option<i64>,
    /// Free disk percentage on the node's data partition. `None`
    /// if never observed (typical for the droplet seen from the
    /// Mac without log-shipping).
    pub disk_free_pct: Option<f64>,
}

impl ClusterState {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn empty() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            nodes: BTreeMap::new(),
            layout_version: 0,
        }
    }
}

/// Path to the cluster TOML config. LD#9 — same resolution as
/// `grants_path()`; honors the `GARAGETYTUS_HOME` override.
pub fn cluster_config_path() -> PathBuf {
    crate::paths::config_dir().join("cluster.toml")
}

/// Path to the cluster state JSON file. Lives in the data dir
/// (it's runtime state, not user config).
pub fn cluster_state_path() -> PathBuf {
    crate::paths::data_dir().join("cluster_state.json")
}

/// Serialize a config to TOML for atomic write. Caller decides
/// where to land the bytes (use `tempfile + rename` for the
/// atomic guarantee).
pub fn serialize_config(cfg: &ClusterConfig) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(cfg)
}

/// Parse a config from TOML.
pub fn parse_config(body: &str) -> Result<ClusterConfig, toml::de::Error> {
    toml::from_str(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn fresh_config() -> ClusterConfig {
        ClusterConfig::new(
            "a".repeat(64),
            "user@droplet.example".to_string(),
            None,
            None,
            None,
        )
    }

    #[test]
    fn cluster_config_round_trips_through_toml() {
        let cfg = fresh_config();
        let body = serialize_config(&cfg).unwrap();
        let parsed = parse_config(&body).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn cluster_config_validate_accepts_fresh() {
        fresh_config().validate().unwrap();
    }

    #[test]
    fn cluster_config_rejects_short_secret() {
        let mut cfg = fresh_config();
        cfg.rpc_secret = "deadbeef".to_string();
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::InvalidSecret { len: 8 })
        ));
    }

    #[test]
    fn cluster_config_rejects_non_hex_secret() {
        let mut cfg = fresh_config();
        cfg.rpc_secret = "Z".repeat(64);
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::InvalidSecret { len: 64 })
        ));
    }

    #[test]
    fn cluster_config_rejects_zone_collision() {
        let mut cfg = fresh_config();
        cfg.droplet_zone = cfg.mac_zone.clone();
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::ZoneCollision { .. })
        ));
    }

    #[test]
    fn cluster_config_rejects_empty_droplet_host() {
        let mut cfg = fresh_config();
        cfg.droplet_host.clear();
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::EmptyDropletHost)
        ));
    }

    #[test]
    fn cluster_config_rejects_invalid_replication_factor() {
        let mut cfg = fresh_config();
        cfg.replication_factor = 0;
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::InvalidReplicationFactor { got: 0 })
        ));
        cfg.replication_factor = 9;
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::InvalidReplicationFactor { got: 9 })
        ));
    }

    #[test]
    fn cluster_config_rejects_schema_mismatch() {
        let mut cfg = fresh_config();
        cfg.schema_version = 99;
        assert!(matches!(
            cfg.validate(),
            Err(ClusterConfigError::SchemaMismatch { got: 99, want: 1 })
        ));
    }

    #[test]
    fn cluster_state_empty_serializes_cleanly() {
        let s = ClusterState::empty();
        let body = serde_json::to_string(&s).unwrap();
        let parsed: ClusterState = serde_json::from_str(&body).unwrap();
        assert_eq!(s, parsed);
        assert_eq!(parsed.layout_version, 0);
        assert!(parsed.nodes.is_empty());
    }

    #[test]
    fn cluster_state_with_nodes_round_trips() {
        let mut s = ClusterState::empty();
        s.nodes.insert(
            "mac".to_string(),
            NodeState {
                reachable: true,
                last_heartbeat_unix_seconds: Some(1714000000),
                disk_free_pct: Some(45.2),
            },
        );
        s.nodes.insert(
            "droplet".to_string(),
            NodeState {
                reachable: false,
                last_heartbeat_unix_seconds: Some(1713999500),
                disk_free_pct: None,
            },
        );
        s.layout_version = 3;
        let body = serde_json::to_string(&s).unwrap();
        let parsed: ClusterState = serde_json::from_str(&body).unwrap();
        assert_eq!(s, parsed);
    }

    #[test]
    fn cluster_paths_honor_override() {
        let _g = LOCK.lock().unwrap();
        std::env::set_var(crate::paths::GARAGETYTUS_HOME_ENV, "/tmp/gtx-cluster-test");
        assert_eq!(
            cluster_config_path(),
            PathBuf::from("/tmp/gtx-cluster-test/config/cluster.toml")
        );
        assert_eq!(
            cluster_state_path(),
            PathBuf::from("/tmp/gtx-cluster-test/data/cluster_state.json")
        );
        std::env::remove_var(crate::paths::GARAGETYTUS_HOME_ENV);
    }
}
