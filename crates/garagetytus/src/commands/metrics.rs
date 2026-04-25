//! `GET /metrics` — LD#11 Prometheus surface.
//!
//! Spawned from `garagetytus serve` alongside the watchdog tick
//! loop. Listens on `127.0.0.1:3904` (garagetytus's own admin
//! port — Garage owns 3903, so we co-exist on the next port up).
//! Each scrape reads the latest `<state-dir>/watchdog.json`
//! atomically, renders Prometheus text format, returns it.
//!
//! Per LD#11, both surfaces ship in v0.1: the JSON file (for
//! CLIs/dashboards that don't speak Prometheus) AND the
//! `/metrics` HTTP endpoint (for Prometheus / Grafana scrapes).
//! No IPC socket — external integrations poll either surface.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use garagetytus_core::ClusterState;
use garagetytus_watchdogs::{
    derive_cluster_mode, read_watchdog_json, Mode, WatchdogState,
};

/// Port garagetytus's admin endpoint binds. 3904 = 3903 + 1, where
/// 3903 is Garage's own admin port.
pub const METRICS_PORT: u16 = 3904;

#[derive(Clone)]
struct MetricsAppState {
    state_dir: Arc<PathBuf>,
}

/// Spawn the metrics server on `127.0.0.1:METRICS_PORT`. Returns
/// when the binding fails or the future is cancelled. Caller is
/// responsible for racing this against the daemon shutdown.
pub async fn serve_metrics(state_dir: PathBuf) -> Result<()> {
    let app_state = MetricsAppState {
        state_dir: Arc::new(state_dir),
    };
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .with_state(app_state);

    let addr: SocketAddr = ([127, 0, 0, 1], METRICS_PORT).into();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("garagetytus metrics: listening on http://{}", addr);
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn metrics_handler(
    State(app_state): State<MetricsAppState>,
) -> impl IntoResponse {
    match read_watchdog_json(&app_state.state_dir) {
        Ok(Some(state)) => {
            // v0.5: optionally read the cluster state to render
            // per-zone + cluster_mode rollup gauges. Absent file =
            // single-node mode (v0.1 behaviour preserved verbatim).
            let cluster_state = read_cluster_state_alongside(&app_state.state_dir);
            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )],
                render_prometheus_with_cluster(&state, cluster_state.as_ref()),
            )
        }
        Ok(None) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            "# garagetytus: watchdog has not run yet — start the daemon\n".to_string(),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            format!("# garagetytus metrics error: {}\n", e),
        ),
    }
}

/// v0.5 cluster state lives at `<data_dir>/cluster_state.json`.
/// Watchdog state lives in the same data_dir, so we resolve from
/// the same path the metrics handler already has. Returns `None`
/// when the file is absent (single-node mode).
fn read_cluster_state_alongside(state_dir: &std::path::Path) -> Option<ClusterState> {
    let p = state_dir.join("cluster_state.json");
    if !p.exists() {
        return None;
    }
    let bytes = std::fs::read(&p).ok()?;
    serde_json::from_slice::<ClusterState>(&bytes).ok()
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

/// v0.5 wrapper — calls into the v0.1 single-node renderer, then
/// appends per-zone + cluster_mode rollup gauges when a cluster
/// state is observed. The v0.1 alias `garagetytus_mode{...}` stays
/// in place verbatim so existing dashboards keep working.
pub fn render_prometheus_with_cluster(
    state: &WatchdogState,
    cluster: Option<&ClusterState>,
) -> String {
    let mut out = render_prometheus(state);
    let Some(cluster) = cluster else {
        return out;
    };

    // Per-node + per-zone gauges (Q6 verdict — primary signal).
    out.push_str(
        "# HELP garagetytus_node_mode Per-node operational mode (1 = active mode, others = 0).\n",
    );
    out.push_str("# TYPE garagetytus_node_mode gauge\n");
    out.push_str(
        "# HELP garagetytus_zone_mode Per-zone operational mode (alias of node_mode for v0.5; diverges in v0.9+ N>2 setups).\n",
    );
    out.push_str("# TYPE garagetytus_zone_mode gauge\n");

    // Convert the cluster's per-zone disk_free_pct into a Mode via
    // hysteresis. v0.5 derivation: disk_free_pct < 10 → ro, ≥ 15
    // → rw, in-between → rw (we don't carry prev_mode across the
    // network so the conservative pick is rw — Garage will flip
    // once the local node's own watchdog ticks). When
    // `disk_free_pct` is `None` (we never observed the peer), we
    // treat it as ro for the rollup — "we don't know" → don't
    // write.
    let mut zone_modes: Vec<(String, Mode)> = Vec::new();
    for (zone, node) in &cluster.nodes {
        let mode = match node.disk_free_pct {
            Some(pct) if pct < 10.0 => Mode::Ro,
            Some(_) => Mode::Rw,
            None => Mode::Ro,
        };
        zone_modes.push((zone.clone(), mode));
        let rw = if mode == Mode::Rw { 1 } else { 0 };
        let ro = if mode == Mode::Ro { 1 } else { 0 };
        out.push_str(&format!(
            "garagetytus_node_mode{{node=\"{z}\",mode=\"rw\"}} {rw}\n",
            z = zone,
            rw = rw
        ));
        out.push_str(&format!(
            "garagetytus_node_mode{{node=\"{z}\",mode=\"ro\"}} {ro}\n",
            z = zone,
            ro = ro
        ));
        out.push_str(&format!(
            "garagetytus_zone_mode{{zone=\"{z}\",mode=\"rw\"}} {rw}\n",
            z = zone,
            rw = rw
        ));
        out.push_str(&format!(
            "garagetytus_zone_mode{{zone=\"{z}\",mode=\"ro\"}} {ro}\n",
            z = zone,
            ro = ro
        ));
    }

    // Q6 strict cluster rollup — derived, not observed.
    let cluster_mode = derive_cluster_mode(&zone_modes);
    out.push_str(
        "# HELP garagetytus_cluster_mode Strict-rollup cluster mode (1 = rw iff EVERY zone rw; else ro).\n",
    );
    out.push_str("# TYPE garagetytus_cluster_mode gauge\n");
    let cluster_rw = if cluster_mode == Mode::Rw { 1 } else { 0 };
    let cluster_ro = if cluster_mode == Mode::Ro { 1 } else { 0 };
    out.push_str(&format!(
        "garagetytus_cluster_mode{{mode=\"rw\"}} {}\n",
        cluster_rw
    ));
    out.push_str(&format!(
        "garagetytus_cluster_mode{{mode=\"ro\"}} {}\n",
        cluster_ro
    ));

    // Layout version + node-reachability counters.
    out.push_str("# HELP garagetytus_cluster_layout_version Garage layout version observed locally.\n");
    out.push_str("# TYPE garagetytus_cluster_layout_version gauge\n");
    out.push_str(&format!(
        "garagetytus_cluster_layout_version {}\n",
        cluster.layout_version
    ));

    let reachable = cluster.nodes.values().filter(|n| n.reachable).count();
    out.push_str("# HELP garagetytus_cluster_reachable_nodes Count of nodes that responded to the last RPC heartbeat.\n");
    out.push_str("# TYPE garagetytus_cluster_reachable_nodes gauge\n");
    out.push_str(&format!(
        "garagetytus_cluster_reachable_nodes {}\n",
        reachable
    ));

    out
}

/// Render a [`WatchdogState`] as Prometheus text format per LD#11.
/// Five gauges/counters surfaced; consumers parse via any standard
/// Prometheus scrape pipeline. v0.1 single-node entry point;
/// cluster mode adds gauges via `render_prometheus_with_cluster`.
pub fn render_prometheus(state: &WatchdogState) -> String {
    let mut out = String::new();
    out.push_str("# HELP garagetytus_disk_free_pct Free space % on the data partition.\n");
    out.push_str("# TYPE garagetytus_disk_free_pct gauge\n");
    out.push_str(&format!(
        "garagetytus_disk_free_pct {:.4}\n",
        state.disk_free_pct
    ));

    out.push_str("# HELP garagetytus_mode Operational mode (1 = active mode, others = 0).\n");
    out.push_str("# TYPE garagetytus_mode gauge\n");
    let rw_value = if state.mode.as_str() == "rw" { 1 } else { 0 };
    let ro_value = if state.mode.as_str() == "ro" { 1 } else { 0 };
    out.push_str(&format!(
        "garagetytus_mode{{mode=\"rw\"}} {}\n",
        rw_value
    ));
    out.push_str(&format!(
        "garagetytus_mode{{mode=\"ro\"}} {}\n",
        ro_value
    ));

    out.push_str("# HELP garagetytus_uptime_seconds Daemon uptime since serve start.\n");
    out.push_str("# TYPE garagetytus_uptime_seconds gauge\n");
    out.push_str(&format!(
        "garagetytus_uptime_seconds {}\n",
        state.uptime_seconds
    ));

    out.push_str("# HELP garagetytus_unclean_shutdown_total Cumulative unclean-shutdown detections (sentinel.lock orphan PIDs).\n");
    out.push_str("# TYPE garagetytus_unclean_shutdown_total counter\n");
    out.push_str(&format!(
        "garagetytus_unclean_shutdown_total {}\n",
        state.unclean_shutdown_total
    ));

    out.push_str("# HELP garagetytus_watchdog_last_tick_unix_seconds Unix timestamp of the most recent watchdog tick.\n");
    out.push_str("# TYPE garagetytus_watchdog_last_tick_unix_seconds gauge\n");
    out.push_str(&format!(
        "garagetytus_watchdog_last_tick_unix_seconds {}\n",
        state.last_tick_unix_seconds
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use garagetytus_watchdogs::Mode;

    fn sample_state() -> WatchdogState {
        WatchdogState {
            mode: Mode::Rw,
            disk_free_pct: 73.5,
            uptime_seconds: 3600,
            unclean_shutdown_total: 2,
            last_tick_unix_seconds: 1_745_600_000,
            data_dir: PathBuf::from("/tmp"),
            schema_version: 1,
        }
    }

    #[test]
    fn render_prometheus_carries_all_five_gauges() {
        let body = render_prometheus(&sample_state());
        for needle in [
            "garagetytus_disk_free_pct ",
            "garagetytus_mode{mode=\"rw\"} 1",
            "garagetytus_mode{mode=\"ro\"} 0",
            "garagetytus_uptime_seconds 3600",
            "garagetytus_unclean_shutdown_total 2",
            "garagetytus_watchdog_last_tick_unix_seconds 1745600000",
        ] {
            assert!(
                body.contains(needle),
                "Prometheus output missing: {}\n--- body ---\n{}",
                needle,
                body
            );
        }
        // Also: every metric must have a # HELP and # TYPE line.
        for metric in [
            "garagetytus_disk_free_pct",
            "garagetytus_mode",
            "garagetytus_uptime_seconds",
            "garagetytus_unclean_shutdown_total",
            "garagetytus_watchdog_last_tick_unix_seconds",
        ] {
            assert!(
                body.contains(&format!("# HELP {} ", metric)),
                "missing # HELP {}",
                metric
            );
            assert!(
                body.contains(&format!("# TYPE {} ", metric)),
                "missing # TYPE {}",
                metric
            );
        }
    }

    #[test]
    fn render_prometheus_flips_mode_label() {
        let mut s = sample_state();
        s.mode = Mode::Ro;
        let body = render_prometheus(&s);
        assert!(body.contains("garagetytus_mode{mode=\"rw\"} 0"));
        assert!(body.contains("garagetytus_mode{mode=\"ro\"} 1"));
    }

    #[test]
    fn render_prometheus_emits_decimal_disk_pct() {
        let mut s = sample_state();
        s.disk_free_pct = 12.345678;
        let body = render_prometheus(&s);
        // Should round to 4 decimal places (chosen for readability).
        assert!(body.contains("garagetytus_disk_free_pct 12.3457"));
    }

    #[tokio::test]
    async fn metrics_handler_returns_503_when_no_watchdog_json() {
        let tmp = tempfile::tempdir().unwrap();
        let app_state = MetricsAppState {
            state_dir: Arc::new(tmp.path().to_path_buf()),
        };
        // Hit the handler directly — no actual HTTP server.
        let resp = metrics_handler(State(app_state.clone()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn metrics_handler_returns_200_when_state_present() {
        let tmp = tempfile::tempdir().unwrap();
        let state = sample_state();
        garagetytus_watchdogs::write_watchdog_json(tmp.path(), &state).unwrap();
        let app_state = MetricsAppState {
            state_dir: Arc::new(tmp.path().to_path_buf()),
        };
        let resp = metrics_handler(State(app_state)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ─── Q6 hybrid verdict — cluster-mode metrics tests ────────

    fn sample_cluster() -> ClusterState {
        let mut s = ClusterState::empty();
        s.layout_version = 4;
        s.nodes.insert(
            "mac".to_string(),
            garagetytus_core::NodeState {
                reachable: true,
                last_heartbeat_unix_seconds: Some(1_745_600_000),
                disk_free_pct: Some(42.0),
            },
        );
        s.nodes.insert(
            "droplet".to_string(),
            garagetytus_core::NodeState {
                reachable: true,
                last_heartbeat_unix_seconds: Some(1_745_599_950),
                disk_free_pct: Some(8.5), // → ro
            },
        );
        s
    }

    #[test]
    fn render_with_cluster_emits_per_zone_and_rollup_gauges() {
        let state = sample_state();
        let cluster = sample_cluster();
        let body = render_prometheus_with_cluster(&state, Some(&cluster));

        // v0.1 alias still present (backward compat).
        assert!(body.contains("garagetytus_mode{mode=\"rw\"} 1"));

        // Per-node and per-zone gauges for both nodes.
        assert!(body.contains("garagetytus_node_mode{node=\"mac\",mode=\"rw\"} 1"));
        assert!(body.contains("garagetytus_node_mode{node=\"mac\",mode=\"ro\"} 0"));
        assert!(body.contains("garagetytus_node_mode{node=\"droplet\",mode=\"rw\"} 0"));
        assert!(body.contains("garagetytus_node_mode{node=\"droplet\",mode=\"ro\"} 1"));
        assert!(body.contains("garagetytus_zone_mode{zone=\"mac\",mode=\"rw\"} 1"));
        assert!(body.contains("garagetytus_zone_mode{zone=\"droplet\",mode=\"ro\"} 1"));

        // Q6 strict rollup: droplet is ro → cluster ro.
        assert!(body.contains("garagetytus_cluster_mode{mode=\"rw\"} 0"));
        assert!(body.contains("garagetytus_cluster_mode{mode=\"ro\"} 1"));

        // Layout version + reachable nodes.
        assert!(body.contains("garagetytus_cluster_layout_version 4"));
        assert!(body.contains("garagetytus_cluster_reachable_nodes 2"));

        // HELP + TYPE lines for the new metrics.
        for metric in [
            "garagetytus_node_mode",
            "garagetytus_zone_mode",
            "garagetytus_cluster_mode",
            "garagetytus_cluster_layout_version",
            "garagetytus_cluster_reachable_nodes",
        ] {
            assert!(
                body.contains(&format!("# HELP {} ", metric)),
                "missing # HELP {}",
                metric
            );
            assert!(
                body.contains(&format!("# TYPE {} ", metric)),
                "missing # TYPE {}",
                metric
            );
        }
    }

    #[test]
    fn render_with_cluster_handles_all_rw_zones() {
        let state = sample_state();
        let mut cluster = sample_cluster();
        // Bump droplet free-pct above the rw threshold.
        cluster
            .nodes
            .get_mut("droplet")
            .unwrap()
            .disk_free_pct = Some(80.0);
        let body = render_prometheus_with_cluster(&state, Some(&cluster));
        // Both zones rw → cluster rw.
        assert!(body.contains("garagetytus_cluster_mode{mode=\"rw\"} 1"));
        assert!(body.contains("garagetytus_cluster_mode{mode=\"ro\"} 0"));
    }

    #[test]
    fn render_without_cluster_state_matches_v0_1() {
        // Cluster state absent → no cluster gauges; v0.1 surface
        // preserved verbatim.
        let state = sample_state();
        let body_v0_1 = render_prometheus(&state);
        let body_v0_5 = render_prometheus_with_cluster(&state, None);
        assert_eq!(body_v0_1, body_v0_5);
        assert!(!body_v0_5.contains("garagetytus_cluster_mode"));
        assert!(!body_v0_5.contains("garagetytus_node_mode"));
    }

    #[test]
    fn render_with_cluster_treats_unobserved_disk_as_ro() {
        // disk_free_pct = None (peer never observed) → per-zone ro.
        let state = sample_state();
        let mut cluster = sample_cluster();
        cluster.nodes.get_mut("droplet").unwrap().disk_free_pct = None;
        let body = render_prometheus_with_cluster(&state, Some(&cluster));
        assert!(body.contains("garagetytus_zone_mode{zone=\"droplet\",mode=\"ro\"} 1"));
        // Mac is rw, droplet is ro (unobserved) → cluster ro (strict).
        assert!(body.contains("garagetytus_cluster_mode{mode=\"ro\"} 1"));
    }

    #[tokio::test]
    async fn metrics_endpoint_real_wire_emits_cluster_metrics_when_present() {
        // Real-wire test that exercises the full pipeline: cluster_state.json
        // is present alongside watchdog.json → the HTTP response must carry
        // the per-zone + cluster_mode gauges.
        let tmp = tempfile::tempdir().unwrap();
        let state = sample_state();
        garagetytus_watchdogs::write_watchdog_json(tmp.path(), &state).unwrap();

        let cluster = sample_cluster();
        let cluster_path = tmp.path().join("cluster_state.json");
        std::fs::write(&cluster_path, serde_json::to_vec(&cluster).unwrap()).unwrap();

        let app_state = MetricsAppState {
            state_dir: Arc::new(tmp.path().to_path_buf()),
        };
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(app_state);

        let listener =
            tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let bound = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app.into_make_service()).await;
        });

        let url = format!("http://{}/metrics", bound);
        let body = reqwest::get(&url).await.unwrap().text().await.unwrap();

        assert!(body.contains("garagetytus_cluster_mode{mode=\"ro\"} 1"));
        assert!(body.contains("garagetytus_node_mode{node=\"mac\",mode=\"rw\"} 1"));
        assert!(body.contains("garagetytus_cluster_layout_version 4"));
        assert!(body.contains("garagetytus_cluster_reachable_nodes 2"));
    }

    /// Real-wire test (`feedback_test_through_real_wire`): bind the
    /// axum server on an ephemeral port, hit it with a real HTTP
    /// client, verify Prometheus text format on the wire. Catches
    /// routing / content-type / serialisation mismatches that the
    /// in-process handler tests would miss.
    #[tokio::test]
    async fn metrics_endpoint_real_wire_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let state = sample_state();
        garagetytus_watchdogs::write_watchdog_json(tmp.path(), &state).unwrap();
        let state_dir = Arc::new(tmp.path().to_path_buf());

        let app_state = MetricsAppState {
            state_dir: state_dir.clone(),
        };
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .with_state(app_state);

        let listener =
            tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let bound_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app.into_make_service()).await;
        });

        // Probe both routes.
        let metrics_url = format!("http://{}/metrics", bound_addr);
        let health_url = format!("http://{}/health", bound_addr);

        let metrics_resp = reqwest::get(&metrics_url).await.unwrap();
        assert_eq!(metrics_resp.status(), 200);
        let ct = metrics_resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            ct.starts_with("text/plain"),
            "unexpected content-type: {}",
            ct
        );
        let body = metrics_resp.text().await.unwrap();
        assert!(body.contains("garagetytus_disk_free_pct"));
        assert!(body.contains("garagetytus_mode{mode=\"rw\"} 1"));
        assert!(body.contains("garagetytus_uptime_seconds 3600"));

        let health_resp = reqwest::get(&health_url).await.unwrap();
        assert_eq!(health_resp.status(), 200);
        assert_eq!(health_resp.text().await.unwrap(), "ok\n");
    }
}
