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

use garagetytus_watchdogs::{read_watchdog_json, WatchdogState};

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
        Ok(Some(state)) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            render_prometheus(&state),
        ),
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

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

/// Render a [`WatchdogState`] as Prometheus text format per LD#11.
/// Five gauges/counters surfaced; consumers parse via any standard
/// Prometheus scrape pipeline.
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
