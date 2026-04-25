# Changelog

All notable changes to garagetytus.

The format is based on [Keep a Changelog](https://keepachangelog.com/);
versions follow [SemVer](https://semver.org/).

## [Unreleased — v0.5 multinode scaffolding] — 2026-04-25

> Scaffolding for the v0.5 multinode sprint
> (`MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/`).
> Phase 0 droplet probes still gate Phase A execution; this
> commit lands the Phase-0-independent type system + CLI shape +
> metrics extension + tests so Phase A integrates cleanly the
> moment Phase 0 results record.

### v0.5 type system + CLI shape (Q4-Q5-Q6 lope-locked)

- **`garagetytus-core::cluster`** — `ClusterConfig` (rpc_secret +
  zones + droplet_host + pod_endpoint + replication_factor) +
  `ClusterState` (per-node liveness + layout version) +
  validation + serde + path resolution. Pure types; no network,
  no SSH, no subprocess. **11 unit tests.**
- **`garagetytus-watchdogs::derive_cluster_mode`** — Q6 hybrid
  verdict implementation. Strict aggregation: cluster `rw` iff
  every zone `rw`. Empty input → conservative `ro` ("we don't
  know, don't write"). Pure function; **6 unit tests** covering
  empty / all-rw / any-ro / all-ro / single-zone /
  three-node-future-proofing cases.
- **`garagetytus cluster {init,status,repair}`** — Q4-locked CLI
  surface. `init` writes `<config_dir>/cluster.toml` atomically
  (preflight only — Phase A.1 SSH orchestration lands once
  Phase 0 outcomes record). `status` reads cluster.toml +
  optional `cluster_state.json`. `repair` orchestration scaffold
  prints the per-node plan; full SSH execution gated on Phase
  A.1. `garagetytus repair` (single-node) shells
  `garage repair tables --yes` directly. **4 unit tests.**
- **`commands/metrics.rs` extension** — Q6 hybrid metric shape.
  Reads optional `cluster_state.json` and renders per-zone +
  per-node + cluster_mode rollup gauges when present; falls
  through to v0.1 single-node format when absent. v0.1
  `garagetytus_mode{...}` alias preserved verbatim. **4 new
  tests** including a real-wire round-trip that binds an
  ephemeral axum server, drops cluster_state.json, hits
  `/metrics` over actual HTTP, and asserts the new gauges land
  on the wire.
- **Phase 0 probe scripts** at `sprint-v0.5/phase0/`. 8 bash
  scripts (one per canonical-sprint probe) + driver
  `probe.sh user@host` + README. Captures outputs to
  `sprint-v0.5/phase0/results/PHASE-0-RESULTS-<date>.md` for
  the operator to copy into the MAKAKOO sprint dir.

### Test totals

- garagetytus workspace: **130 pass, 0 fail, 0 warnings**
  (60 lib + 1 contract + 17 core + 35 grants + 17 watchdogs;
  +25 vs rc2 from v0.5 scaffolding). One pre-existing
  parallel-runner flake on
  `garagetytus_grants::audit::tests::query_spans_rotated_archives`
  passes in isolation; identical pattern to the documented
  v0.1-era flake.
- garagetytus-sdk: 15 pass, 0 fail (unchanged).
- makakoo-os workspace: unchanged (v0.5 changes are all
  garagetytus-side; Makakoo wrapper still execs `garagetytus
  bucket *` per Q2 verdict).

### Sprint state

- Drafted: `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/SPRINT.md`
  (carve-out wrapper around the canonical 802-LOC v0.8 sprint
  pi+qwen round-1 hardened).
- Locked: `verdicts/Q4-Q5-Q6-LOCKED.md` (lope round 2026-04-25,
  pi + codex parallel; Q4 + Q5 unanimous A; Q6 hybrid).
- Pending Phase 0: 8 probe scripts ready to run against a real
  droplet. Operator action.
- Phase A blocked on Phase 0 results recording.

## [v0.1.0-rc2 — AC8 auto-repair landed] — 2026-04-25

> Status: workspace + bucket + grants + audit + rate-limit + AGPL
> surface + install + start + bootstrap + watchdogs + CI matrix +
> AC8 auto-repair all landed. **Tag v0.1.0 issued only after a
> clean-host acceptance run** (AC2 idempotence on macOS + Linux,
> AC3 service registration with reboot, AC8 unclean-shutdown
> recovery E2E).

### AC8 auto-repair (Q3 verdict — pi+codex 2026-04-25)

- **`bootstrap::auto_repair_if_single_node(cfg_path)`** — async helper
  that, when `preflight_unclean_check` reported an orphan-PID
  sentinel, waits for garage health (15 s budget @ 500 ms poll),
  probes cluster size via `GET /v1/cluster/layout`, and shells
  `garage -c <cfg> repair tables --yes` iff the cluster is
  single-node. Returns `RepairOutcome::{RepairRan,
  SkippedMultiNode{nodes}, HealthTimeout}` for diagnostic logging.
- **`commands/start.rs::serve`** — captures the preflight result
  into `needs_auto_repair`; when set, spawns a fire-and-forget
  thread (own tokio runtime) that runs the repair flow alongside
  the existing watchdog + metrics threads. Repair never blocks
  startup — every error path soft-fails with `tracing::warn!`.
- **Multi-node guard** — codex's smell test pre-installed for
  v0.5+ topologies. v0.1 always emits 1 node; the guard becomes
  load-bearing only when multi-node clusters land.
- **No flag, no opt-in** — pi's spec-compliance argument carried
  ("integrity probe runs `garage repair`"). Operator surface is
  zero; the auto-repair runs invisibly when it should and
  auto-skips when it shouldn't.
- **4 new unit tests** — `node_count_from_layout` on
  single/multi/empty/missing/wrong-type fixtures + `RepairOutcome`
  variant distinctness. Real-wire E2E deferred to AC8 acceptance
  recipe in `verdicts/Q3-AC8-RECOVERY.md`.

### Carved from Makakoo v0.7.1

- **`garagetytus-grants` crate** — `user_grants`, `rate_limit`,
  `audit`, `audit_escape` (1734 LOC carved verbatim from
  `makakoo-os/makakoo-core/src/capability/`). 35 unit tests.
  Schema version 1 frozen across both repos; drift fixtures
  vendored at `tests/fixtures/`.
- **`garagetytus-core` crate** — cross-platform path resolution
  via the `dirs` crate + `keyring`-backed `SecretsStore` (carved
  from `makakoo/src/secrets.rs`, 137 LOC) + `StorageBackend`
  trait (LD#13 — backend abstraction at workspace top, single
  `GarageBackend` impl in v0.1). 5 unit tests.
- **`garagetytus` CLI crate** — bucket lifecycle business logic
  carved verbatim from `makakoo/src/commands/bucket.rs` (1300 LOC,
  20 lib tests + 3 parse_duration tests + 1 contract test). Only
  rename: `makakoo_core::capability::*` → `garagetytus_grants::*`.
- **`garagetytus-watchdogs` crate** — disk-watch (10/15
  hysteresis via `sysinfo`), integrity-check (sentinel.lock +
  unclean-shutdown counter), keychain-migrate (legacy
  `makakoo-s3-service` → `s3-service`). LD#11 protocol writes
  `<state-dir>/watchdog.json` atomically. 8 unit tests.
- **`commands/metrics.rs`** — LD#11 Prometheus `GET /metrics`
  HTTP endpoint on `127.0.0.1:3904` (garagetytus's own admin
  port; Garage owns 3903). Spawned alongside the watchdog
  tick loop from `garagetytus serve` on its own tokio runtime.
  Reads the latest `watchdog.json` per scrape; emits five
  gauges/counters: `garagetytus_disk_free_pct`,
  `garagetytus_mode{mode="rw|ro"}`, `garagetytus_uptime_seconds`,
  `garagetytus_unclean_shutdown_total`,
  `garagetytus_watchdog_last_tick_unix_seconds`. 5 unit tests.
- **`garagetytus-sdk` (Python pip package)** — carved 376 LOC
  from `lib-harvey-core/src/core/s3/`. 15 tests.
  Cross-platform credential storage via `keyring` package
  (LD#5/6); brand renames (`makakoo` → `garagetytus`,
  `MAKAKOO_PEER_NAME` retained as legacy fallback).

### Subcommands (real impls)

- `garagetytus install` — Mac path detects brew + `garage` and
  generates plist via hand-rolled template (LD#3 fallback);
  Linux path downloads upstream musl binary, SHA-verifies,
  generates systemd-user unit; Windows prints v0.2 deferral.
  Idempotent (AC2).
- `garagetytus uninstall [--keep-data]` — stops daemon (best-
  effort), removes plist / systemd unit, deletes `s3-service`
  keychain entry, removes config + logs (and data unless
  `--keep-data`). Idempotent — second invocation is a no-op
  (AC2 full).
- `garagetytus start / stop / status / restart / serve` —
  `launchctl bootstrap/bootout` (Mac) + `systemctl --user`
  (Linux). `serve` runs garage in foreground + spawns the
  watchdog tick loop in a background thread.
- `garagetytus bootstrap` — calls Garage admin API (`/v1/health`,
  `/v1/cluster/layout`, `/v1/cluster/layout/apply`) for
  single-node layout; provisions `s3-service` keypair via
  `garage key create`; writes creds to OS keychain.
- `garagetytus about` — AGPL surface (Phase B.5).
- `garagetytus bucket {create, list, info, grant, revoke,
  expire, deny-all}` — verbatim carve from Makakoo bucket.rs.

### AGPL posture

- `THIRD_PARTY_NOTICES` ships Garage attribution + AGPL upstream
  source URL + tarball SHA.
- `versions.toml` records the SHA pin map.
- `garagetytus about` subcommand surfaces the same values.
- `tests/contract_no_garage_crates.rs` — LD#1 hard-fail gate
  (zero `garage-*` crate dependency at any level).
- `.github/workflows/ci.yml` AGPL-grep job — fails on any
  `use garage_*` / `extern crate garage_*` in source.

### Cross-platform install

- `install/install.sh` — Linux + macOS web bootstrap.
- `install/install.ps1` — Windows v0.2 deferral notice
  bootstrap (exits 0).
- `install/homebrew-tap.rb` — Homebrew formula source
  (`depends_on "garage"` so brew compiles AGPL source on Mac).

### Docs

- `README.md` — install + usage primer.
- `docs/install/{macos,linux,windows}.md` — per-OS setup.
- `docs/usage/quickstart.md` + `docs/usage/grants.md`.
- `docs/integrate/{makakoo,tytus,external-app}.md` —
  integration contracts.
- `LICENSE` (MIT) + `THIRD_PARTY_NOTICES` (Garage AGPL).

### Cross-platform CI matrix (Phase B.4)

`.github/workflows/ci.yml`:
- macos-latest + ubuntu-latest matrix runs `cargo build
  --workspace --all-targets` + `cargo test --workspace`.
- Separate `pytest` job for `sdk/python/`.
- LD#1 contract test runs separately on every PR.
- AGPL-grep belt-and-suspenders job.
- `cargo-deny check bans licenses advisories` — LD#1 third
  gate (resolver-layer dep ban) + permissive-license allowlist
  + RustSec advisory check. `deny.toml` at repo root pins the
  config.

### Verdicts (lope, pi+codex)

- **Q1** — both PASS Option A (Mac via Homebrew, Linux via
  upstream musl, Windows v0.2). See `MAKAKOO/development/sprints/
  queued/GARAGETYTUS-V0.1/verdicts/Q1-VERDICT.md`.
- **Q2** — both PASS Option A (Makakoo `bucket *` becomes a
  thin wrapper that exec's `garagetytus bucket *` with inherited
  stdio). See `Q2-VERDICT.md`.
- **Q3** — pi=B (auto-run unconditionally), codex=C (opt-in flag).
  Locked: B with codex's multi-node guard pre-installed —
  default-on for single-node clusters (today's v0.1 reality),
  auto-skipped on multi-node. No flag surface. See
  `Q3-AC8-RECOVERY.md`.

### Companion commits on `github.com/makakoo/makakoo-os`

- `b723ef0` — Phase A.5 user_grants shim.
- `9573007` — Phase A.1 rate_limit + audit + audit_escape shims.
- `ae97464` — Phase D Makakoo bucket wrapper (Q2 Option A).

### Test totals

- garagetytus workspace: **105 pass, 0 fail, 0 warnings**
  (51 lib + 1 contract + 6 core + 35 grants + 12 watchdogs;
  +4 vs rc1 from AC8 unit additions).
- garagetytus-sdk: 15 pass, 0 fail.
- makakoo-os workspace: 670 lib pass + 8 wrapper bin pass + 2
  pre-existing TOML failures (predate carve, reproduce on
  pristine HEAD).

### Locally smoke-verified (this session, Mac)

- **AC2 install/uninstall idempotence** ✅ — two install +
  two uninstall round-trips on `GARAGETYTUS_HOME=/tmp/...`,
  config tokens preserved, second uninstall no-op'd.
- **AC6 per-OS path conventions** ✅ — env override routes
  config/data/logs to `/tmp/...`; plist correctly lands in
  `~/Library/LaunchAgents/` (OS-mandated location).
- **AC11 AGPL surface** ✅ — `garagetytus about` prints
  bundled Garage version, upstream source URL, license,
  tarball SHA verbatim.

### Pending for v0.1.0 (non-rc) tag

- **AC8 E2E run** on a clean host (kill -9 garagetytus serve,
  restart, sentinel.lock orphan-PID detection increments
  `unclean_shutdown_total`, auto-repair flow shells
  `garage repair tables`). All code paths are in place;
  empirical verification is Sebastian-side per the recipe in
  `verdicts/Q3-AC8-RECOVERY.md`.
- **AC3** — service registration with reboot survival on
  macOS + Linux.
- **AC2 Linux** — Mac smoke-verified locally; Linux pending.
- **AC4 / AC5 / AC7 / AC9 / AC10 / AC12** — require
  bootstrapped running daemon + (in some cases) Linux box.
- **Phase C.3 Makakoo Python re-export flip** — gated on
  PyPI publish (codex consumption-boundary contract).
- **Phase E (tytus team)** — separate repo, contract is in
  `docs/integrate/tytus.md`.
