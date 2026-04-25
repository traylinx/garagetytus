# Changelog

All notable changes to garagetytus.

The format is based on [Keep a Changelog](https://keepachangelog.com/);
versions follow [SemVer](https://semver.org/).

## [Unreleased — Phase A–F carve-out, 2026-04-25]

> Status: workspace + bucket + grants + audit + rate-limit + AGPL
> surface + docs all landed. **Tag pending** until the install /
> start / watchdog modules ship real (vs stub) implementations
> + cross-platform CI matrix is wired in.

### Carved from Makakoo v0.7.1

- **`garagetytus-grants` crate** — `user_grants`, `rate_limit`,
  `audit`, `audit_escape` (1734 LOC carved verbatim from
  `makakoo-os/makakoo-core/src/capability/`). 35 unit tests pass.
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
  Stub install/start/bootstrap commands; real implementations
  arrive alongside the release pipeline.
- **`garagetytus-sdk` (Python pip package)** — carved 376 LOC
  from `lib-harvey-core/src/core/s3/`. 15 tests pass.
  Cross-platform credential storage via `keyring` package
  (LD#5/6); brand renames (`makakoo` → `garagetytus`,
  `MAKAKOO_PEER_NAME` retained as legacy fallback).

### AGPL posture

- `THIRD_PARTY_NOTICES` ships Garage attribution + AGPL upstream
  source URL + tarball SHA.
- `versions.toml` records the SHA pin map.
- `garagetytus about` subcommand surfaces the same values.
- `tests/contract_no_garage_crates.rs` — LD#1 hard-fail gate
  (zero `garage-*` crate dependency at any level).

### Cross-platform install

- `install/install.sh` — Linux + macOS web bootstrap (downloads
  garagetytus binary from GitHub releases + drops on PATH).
- `install/install.ps1` — Windows v0.2 deferral notice
  bootstrap.
- `install/homebrew-tap.rb` — Homebrew formula source. Declares
  `depends_on "garage"` so the upstream brew formula compiles
  AGPL source on macOS (Mac path per LD#7 amended).

### Docs

- `README.md` — install + usage primer.
- `docs/install/{macos,linux,windows}.md` — per-OS setup.
- `docs/usage/quickstart.md` + `docs/usage/grants.md`.
- `docs/integrate/{makakoo,tytus,external-app}.md` — integration
  contracts for downstream consumers.
- `LICENSE` (MIT) + `THIRD_PARTY_NOTICES` (Garage AGPL).

### Verdicts (lope, pi+codex)

- **Q1** (`MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/
  verdicts/Q1-VERDICT.md`) — both PASS Option A: Mac via
  Homebrew, Linux via upstream binary, Windows v0.2.
- **Q2** (`Q2-VERDICT.md`) — both PASS Option A: Makakoo
  `bucket *` becomes a thin wrapper that shell-outs to
  `garagetytus bucket *` with inherited stdio.

### Companion commits on `github.com/makakoo/makakoo-os`

- `b723ef0` — Phase A.5: re-export `user_grants` shim.
- `9573007` — Phase A.1: re-export `rate_limit` + `audit` +
  `audit_escape` shims.
- `ae97464` — Phase D: Makakoo bucket wrapper (Option A).

### Pending for v0.1 tag

- **Phase B real implementations** — install.rs (Linux musl
  download + SHA-verify + plist/systemd template generation),
  start.rs (launchctl + systemctl orchestration), bootstrap.rs
  (admin-API layout assignment).
- **LD#11 watchdog protocol** — `/metrics` endpoint + atomic
  `<state-dir>/watchdog.json` writer.
- **Phase B.4 cross-platform CI matrix** — GH Actions macos +
  ubuntu runners.
- **Phase C.3 Makakoo Python re-export flip** — deferred to
  post-PyPI publish per codex consumption-boundary contract.
- **Phase E** — tytus-side `tytus bucket` subcommand +
  tray-menu cell. Requires modifications to the separate
  `github.com/traylinx/tytus` repo; spec is in
  `docs/integrate/tytus.md`, contract is ready, tytus team
  picks it up.
- **Acceptance contract** AC2/AC3/AC4/AC5/AC6/AC7/AC8/AC9 all
  require the real install/start/watchdog modules + a clean-host
  E2E run before tag.
