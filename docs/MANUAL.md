# garagetytus — Operator Manual (v0.1.0-rc2)

A single self-contained reference for installing, running, and
recovering a garagetytus daemon on a development laptop. Read this
end-to-end the first time; bookmark §11 (troubleshooting) and §13
(configuration reference) for day-to-day lookups.

> **Audience.** Developers running an S3-compatible store on
> `127.0.0.1:3900` for local agent work. Not production. Not
> multi-node. Single-machine, single-user, ~1 GiB nominal capacity.

---

## Table of contents

1. [What garagetytus is](#1-what-garagetytus-is)
2. [Architecture in 60 seconds](#2-architecture-in-60-seconds)
3. [Install — macOS](#3-install--macos)
4. [Install — Linux](#4-install--linux)
5. [Install — Windows](#5-install--windows)
6. [First-run bootstrap](#6-first-run-bootstrap)
7. [Daily operations](#7-daily-operations)
8. [Bucket + grant lifecycle](#8-bucket--grant-lifecycle)
9. [Observability — `/metrics` + `watchdog.json`](#9-observability--metrics--watchdogjson)
10. [Recovery from unclean shutdown (AC8)](#10-recovery-from-unclean-shutdown-ac8)
11. [Troubleshooting matrix](#11-troubleshooting-matrix)
12. [Integrating with Makakoo + tytus + your own app](#12-integrating-with-makakoo--tytus--your-own-app)
13. [Configuration reference](#13-configuration-reference)
14. [Uninstall](#14-uninstall)
15. [Versioning, upgrades, AGPL posture](#15-versioning-upgrades-agpl-posture)

---

## 1. What garagetytus is

A single-binary daemon wrapper around [Garage](https://garagehq.deuxfleurs.fr/),
the AGPL-3.0 S3-compatible object store. garagetytus owns:

- The cross-platform installer (Homebrew on Mac, web bootstrap on
  Linux).
- Daemon lifecycle (`launchd` on Mac, `systemd --user` on Linux).
- A bucket + grants surface (`garagetytus bucket create / grant /
  revoke`) that issues per-app SigV4 sub-keypairs with TTLs.
- Three baked-in watchdogs (disk-space hysteresis, integrity
  sentinel, legacy-keychain migrate).
- An LD#11 observability protocol (Prometheus `/metrics` HTTP +
  atomic `watchdog.json` mirror).

Garage stays a child process. **Never linked.** AGPL boundary stays
clean (LD#1) — you can ship MIT/Apache code that *uses* garagetytus
without inheriting the AGPL viral clause.

The same `garagetytus bucket grant ...` JSON output works as
drop-in S3 credentials for boto3, aws-cli, rclone, pandas, Logseq
S3 sync, Obsidian Sync, anything that speaks SigV4.

---

## 2. Architecture in 60 seconds

```
┌────────────────────────────────────────────────────────┐
│                    your laptop                         │
│                                                        │
│   ┌──────────────────────────────────────┐             │
│   │   garagetytus (this binary)          │             │
│   │     ├── install / uninstall          │             │
│   │     ├── start / stop / status        │             │
│   │     ├── bootstrap                    │             │
│   │     ├── bucket {create,grant,...}    │             │
│   │     ├── /metrics @ :3904  ◄── you    │             │
│   │     └── watchdogs (own thread)       │             │
│   └─────────────┬────────────────────────┘             │
│                 │ spawns + supervises                  │
│                 ▼                                      │
│   ┌──────────────────────────────────────┐             │
│   │   garage (AGPL upstream binary)      │             │
│   │     ├── S3 API @ :3900   ◄── boto3   │             │
│   │     ├── RPC @ :3901                  │             │
│   │     └── admin API @ :3903            │             │
│   └──────────────────────────────────────┘             │
│                                                        │
│   keychain ◄── service: "garagetytus"                  │
│                accounts: "s3-service" + grant ids      │
└────────────────────────────────────────────────────────┘
```

Four ports, one process tree, one keychain namespace. The AGPL
boundary lives at the subprocess fence — garagetytus speaks to
garage over HTTP (admin API) and the `garage` CLI (subprocess), and
nothing else.

Path layout (default; overridable via `GARAGETYTUS_HOME`):

| OS | data | config | logs |
|---|---|---|---|
| macOS | `~/Library/Application Support/garagetytus/` | (same) | `~/Library/Logs/garagetytus/` |
| Linux | `$XDG_DATA_HOME/garagetytus/` | `$XDG_CONFIG_HOME/garagetytus/` | `<data>/logs/` |

Override:

```bash
export GARAGETYTUS_HOME=/tmp/gtx-test
# → /tmp/gtx-test/{config,data,logs}/
```

---

## 3. Install — macOS

**Prereq.** Homebrew on PATH. Garage upstream ships no Mac binary,
so we compile from source via the `garage` formula (Q1 verdict
2026-04-25). First install takes ~3–5 min for the rust compile;
subsequent updates are cached.

```bash
brew install traylinx/tap/garagetytus
garagetytus install
```

What `garagetytus install` does on Mac:

1. Verifies `garage` is on PATH (errors clearly if not — re-run
   `brew install garage` if missing).
2. Generates a fresh `garagetytus.toml` under
   `~/Library/Application Support/garagetytus/` with random tokens:
   `rpc_secret` (32 hex), `admin_token` + `metrics_token`
   (URL-safe base64, no padding).
3. Writes `~/Library/LaunchAgents/com.traylinx.garagetytus.plist`
   carrying `KeepAlive`, `RunAtLoad`, `StandardOutPath`,
   `StandardErrorPath`, `ProcessType=Background`.
4. Idempotent — re-running preserves existing tokens and skips
   already-present files. Safe to run twice.

Verify:

```bash
garagetytus about               # AGPL surface, version pin, source URL
garagetytus status              # "stopped" until you start
ls ~/Library/LaunchAgents/com.traylinx.garagetytus.plist
```

Then bring the daemon up — see §6.

---

## 4. Install — Linux

```bash
curl -fsSL https://garagetytus.dev/install | sh
garagetytus install
```

What `garagetytus install` does on Linux:

1. Downloads the upstream Garage musl binary
   (`x86_64-unknown-linux-musl` or `aarch64-unknown-linux-musl`,
   per `versions.toml`).
2. SHA-256 verifies against the pinned hash. Refuses to proceed on
   mismatch.
3. `chmod 0755` and drops into `~/.local/bin/garage`.
4. Generates `~/.config/garagetytus/garagetytus.toml` with random
   tokens (same as Mac).
5. Writes `~/.config/systemd/user/garagetytus.service` with
   `Restart=on-failure`, `RestartSec=5s`,
   `StandardOutput=journal`.

Verify:

```bash
garage --version                  # confirms binary works
systemctl --user status garagetytus   # "loaded; inactive (dead)"
```

---

## 5. Install — Windows

**v0.2 deferral.** Garage upstream ships no Windows binary; v0.1
budget can't carry a Windows compile-from-source pipeline. The
Windows installer (`install.ps1`) prints a deferral notice and
exits 0. Reopens at v0.2.

If you need garagetytus on Windows today, run it under WSL2 with
the Linux install path.

---

## 6. First-run bootstrap

Two-command sequence after install:

```bash
garagetytus start         # daemon comes up on :3900 / :3901 / :3903
garagetytus bootstrap     # admin-API layout + service keypair
```

What `garagetytus bootstrap` does:

1. Reads `admin_token` + `api_bind_addr` from
   `garagetytus.toml`.
2. `GET /v1/health` — confirms daemon is responsive (5s timeout).
3. `GET /v1/cluster/layout` — checks for existing role assignments.
   If found, skips to step 6 (idempotent).
4. `POST /v1/cluster/layout` — assigns the local node a role
   (`zone="local"`, `capacity=1 GiB nominal`, `tags=["local"]`).
5. `POST /v1/cluster/layout/apply?version=N` — commits.
6. Provisions an `s3-service` keypair via `garage key create
   s3-service`.
7. Writes the access/secret pair to the OS keychain at
   `(service="garagetytus", account="s3-service")` as a JSON blob:
   `{access_key, secret_key, endpoint:"http://127.0.0.1:3900"}`.

Idempotent — re-running detects the existing layout and keypair
and skips. Safe to run after a service-keypair rotation.

Verify:

```bash
curl -s http://127.0.0.1:3904/metrics | grep mode
# garagetytus_mode{mode="rw"} 1   ← daemon is healthy
```

---

## 7. Daily operations

```bash
garagetytus start         # bootstrap launchd / systemctl
garagetytus stop          # bootout / stop unit
garagetytus restart       # stop + start
garagetytus status        # current state (running pid, state)
garagetytus serve         # foreground; Ctrl-C to stop
```

`serve` is for users who supply their own supervisor (Docker, k8s,
runit, manual launchctl). It runs `garage -c <cfg> server`
synchronously in the foreground and spawns the watchdog +
metrics threads alongside.

Per-OS exit-code tolerance you may see in scripts:

- **launchctl bootstrap exit 17** — already loaded (success).
- **launchctl bootout exit 3 / 36** — already stopped or no such
  service (success).
- **systemctl exit codes** — pass through verbatim.

---

## 8. Bucket + grant lifecycle

### Create a bucket

```bash
garagetytus bucket create my-data --ttl 7d --quota 1G
```

| Flag | Grammar | Notes |
|---|---|---|
| `--ttl` | `30m / 1h / 24h / 7d / permanent` | Required. `permanent` skips watchdog expiry. |
| `--quota` | `100M / 1G / 10G / unlimited` | `unlimited` requires `--confirm-yes-really`. |

### List buckets

```bash
garagetytus bucket list                  # human table
garagetytus bucket list --json           # machine envelope
```

### Mint a per-app grant

```bash
garagetytus bucket grant my-data \
    --to "my-python-app" \
    --perms read,write \
    --ttl 1h \
    --json
```

Output envelope:

```json
{
  "grant_id": "g_20260425_a3f9c12d",
  "access_key": "GK1234567890abcdef",
  "secret_key": "...",
  "endpoint_url": "http://127.0.0.1:3900",
  "expires_at": "2026-04-25T13:42:00Z"
}
```

Wire those four values into your S3 client. The grant carries an
Ed25519-signed entry in the `grants.json` store at the
LD#9-canonical config path (see §13). Makakoo and tytus consume
the same store read-only without re-implementing.

### Revoke / expire / deny-all

```bash
garagetytus bucket revoke g_20260425_a3f9c12d
garagetytus bucket expire my-data         # for TTL'd buckets
garagetytus bucket deny-all my-data --ttl 1h    # emergency stop
```

Revocation is atomic 3-state (`active → revoking → revoked`). If
the backend key-delete fails, the watchdog retries every 60s.
`deny-all` flips a Garage flag that 403s every read/write
including those carrying still-valid presigned URLs — use it when
you suspect a leak. `expire` calls `deny-all` *before* `revoke`
so no presigned URL races the key delete.

---

## 9. Observability — `/metrics` + `watchdog.json`

Two surfaces, same data, different consumers (LD#11):

```bash
# Prometheus — for Grafana, alertmanager, dashboards.
curl -s http://127.0.0.1:3904/metrics
```

Five gauges/counters with `# HELP` + `# TYPE` lines:

| Metric | Meaning |
|---|---|
| `garagetytus_disk_free_pct` | Free space on the data partition (10/15 hysteresis triggers `mode` flip) |
| `garagetytus_mode{mode="rw"\|"ro"}` | Read-write or read-only (auto-flipped under disk pressure) |
| `garagetytus_uptime_seconds` | Since `serve` started |
| `garagetytus_unclean_shutdown_total` | Monotonic counter — see §10 |
| `garagetytus_watchdog_last_tick_unix_seconds` | Last successful watchdog tick |

```bash
# JSON mirror — for CLIs, dashboards, Tytus tray, polling scripts.
cat ~/Library/Application\ Support/garagetytus/watchdog.json    # Mac
cat ~/.local/share/garagetytus/watchdog.json                    # Linux
```

The `/metrics` endpoint returns **HTTP 503** when `watchdog.json`
is missing (daemon never ran cleanly). Treat 503 as "wait + retry"
during cold start, not as a permanent failure.

---

## 10. Recovery from unclean shutdown (AC8)

`garagetytus serve` writes a sentinel (`sentinel.lock` carrying
the current PID) every tick. If the process exits without
cleaning the sentinel — `kill -9`, OOM kill, power loss — the
next `garagetytus serve` invocation:

1. Notices the orphan PID at preflight.
2. Increments `garagetytus_unclean_shutdown_total` (persisted to
   `unclean_shutdown_total.txt` so it survives across restarts).
3. Logs a stderr warning.
4. Spawns garage normally.
5. **Auto-runs `garage repair tables --yes`** post-spawn iff the
   cluster has exactly one node (the v0.1 default).

Per Q3 lope verdict (pi+codex 2026-04-25), the auto-repair flow
is default-on for single-node clusters and auto-skipped on
multi-node (where `repair tables` semantics diverge across
network partitions). No flag, no operator ceremony.

The flow has a **15-second health-check budget** before giving up.
Any failure (timeout, network, 4xx, 5xx) is logged via
`tracing::warn!` and swallowed — repair is best-effort and
**never blocks startup**.

Manual recovery (rarely needed):

```bash
# After garagetytus serve is running:
garage -c <config-path> repair tables --yes        # safe, sub-second
garage -c <config-path> repair start --all --yes   # heavy, hours
```

The `--all` form is for multi-node restoration after disk loss.
Don't run it on a single-node laptop unless you actually have
detected corruption — it's a long-running pause for nothing.

Inspect the counter:

```bash
curl -s http://127.0.0.1:3904/metrics | grep unclean_shutdown_total
# garagetytus_unclean_shutdown_total <count>
```

---

## 11. Troubleshooting matrix

| Symptom | Likely cause | Fix |
|---|---|---|
| `garagetytus install: garage not on PATH` | Missing prereq | Mac: `brew install garage`. Linux: re-run `curl ... \| sh`. |
| `port collision — refusing to start` | 3900 / 3901 / 3903 / 3904 already bound | Inspect the PID hint in the error; kill or remap in `garagetytus.toml`. |
| `daemon not responding at <admin-url>` during bootstrap | `start` not run yet, or service still warming up | Wait 2–3 s and retry; verify `garagetytus status`. |
| `previous run did not exit cleanly` on every restart | sentinel.lock not cleaned (kill -9, OOM, panic) | Expected. Check `unclean_shutdown_total` is incrementing once per event, not per restart. If it climbs without unclean events, file an issue. |
| `garagetytus_mode{mode="ro"} 1` on `/metrics` | Disk free below 10% → hysteresis flipped to read-only | Free space; mode flips back at 15%. |
| `503` from `/metrics` | `watchdog.json` missing | Daemon never ran — `garagetytus start` then wait one tick (~30s). |
| `garage key create` fails with permission error | Stale Garage layout from a prior install | `garagetytus uninstall --keep-data` then `install` + `bootstrap` again. |
| `boto3` returns `400 Bad Request` | Default virtual-host addressing | `Config(s3={"addressing_style": "path"})`. Mandatory (LD#4). |
| `boto3` returns `403 Forbidden` on PUT | Grant lacks `write` perm | `garagetytus bucket grant <bucket> --perms read,write`. |
| `boto3` returns `403` on every request right after `revoke` | Grant deleted; existing client still using cached creds | Recreate the boto3 client with the next grant's creds. |
| launchd plist won't load (Mac) | Old plist from prior version | `launchctl bootout gui/$(id -u)/com.traylinx.garagetytus` then `garagetytus install`. |
| systemd unit won't start (Linux) | User services disabled at session level | `systemctl --user daemon-reload && loginctl enable-linger $USER`. |

For symptoms not on this matrix: open an issue with the output of
`garagetytus about`, `garagetytus status`, and the last 50 lines
of `<log_dir>/garagetytus.log`.

---

## 12. Integrating with Makakoo + tytus + your own app

### Makakoo

Makakoo's `bucket` subcommands shell out to `garagetytus bucket
*` with inherited stdio (Q2 verdict). Single source of truth for
the grants store; no schema drift; `--json` envelopes pass
through verbatim.

```bash
makakoo bucket list                # delegates to garagetytus
makakoo bucket grant my-data --to ... --json | jq .
```

Per-agent isolation, presigned URLs, the `harvey_bucket_*` MCP
tools — all unchanged. The daemon is now garagetytus instead of a
Makakoo-managed Garage process; agents don't notice.

### tytus

Separate private repo (`github.com/traylinx/tytus`) — Mac binary
at `/usr/local/bin/tytus`. tytus consumes the same `grants.json`
read-only over the WireGuard tunnel via the MCP shim. Spec-only
contract for v0.1; tytus team picks up the implementation. See
`docs/integrate/tytus.md`.

### Your own app (boto3)

```python
import json, keyring, boto3
from botocore.config import Config

creds = json.loads(keyring.get_password("garagetytus", "s3-service"))
s3 = boto3.client(
    "s3",
    endpoint_url=creds["endpoint"],
    region_name="garage",
    aws_access_key_id=creds["access_key"],
    aws_secret_access_key=creds["secret_key"],
    config=Config(s3={"addressing_style": "path"}),
)
s3.put_object(Bucket="my-data", Key="hello.txt", Body=b"hi")
```

Or use the bundled Python SDK:

```bash
pip install garagetytus-sdk         # once published to PyPI
```

```python
from garagetytus import client

s3 = client()                                    # service identity
s3 = client(grant_id="g_20260425_a3f9c12d")      # per-grant scope
```

---

## 13. Configuration reference

### File paths

| Item | macOS | Linux |
|---|---|---|
| Config file | `~/Library/Application Support/garagetytus/garagetytus.toml` | `~/.config/garagetytus/garagetytus.toml` |
| Grants store (LD#9) | `~/Library/Application Support/garagetytus/grants.json` | `~/.config/garagetytus/grants.json` |
| Data dir | `~/Library/Application Support/garagetytus/` | `~/.local/share/garagetytus/` |
| Log dir | `~/Library/Logs/garagetytus/` | `~/.local/share/garagetytus/logs/` |
| Watchdog state | `<data_dir>/watchdog.json` | (same) |
| Sentinel lock | `<data_dir>/sentinel.lock` | (same) |
| Unclean counter | `<data_dir>/unclean_shutdown_total.txt` | (same) |
| Service unit | `~/Library/LaunchAgents/com.traylinx.garagetytus.plist` | `~/.config/systemd/user/garagetytus.service` |

### Environment variables

| Var | Effect |
|---|---|
| `GARAGETYTUS_HOME` | Collapses every directory to `<dir>/{config,data,logs}/`. Useful for tests + container deployments. |
| `GARAGETYTUS_WATCHDOG_INTERVAL_S` | Override the 30s default tick cadence. |
| `RUST_LOG` | Standard `tracing-subscriber` filter (`garagetytus=debug,reqwest=info`). |

### Port allocations

| Port | Owner | Bind |
|---|---|---|
| 3900 | Garage S3 API | `127.0.0.1` |
| 3901 | Garage RPC | `127.0.0.1` |
| 3903 | Garage admin API | `127.0.0.1` |
| 3904 | garagetytus `/metrics` | `127.0.0.1` |

To remap, edit `garagetytus.toml` (`[s3_api]`, `[rpc]`, `[admin]`
sections) and the metrics port in `~/.garagetytus/metrics.toml` —
only do this if you have a hard collision; the defaults are
chosen to avoid common dev tooling.

### Keychain entries

| Service | Account | Payload |
|---|---|---|
| `garagetytus` | `s3-service` | `{access_key, secret_key, endpoint}` JSON |
| `garagetytus` | `g_<grant_id>` | (per-grant, optional, only when `--store-creds`) |

Inspect on Mac:

```bash
security find-generic-password -s garagetytus -a s3-service -w
```

---

## 14. Uninstall

```bash
garagetytus uninstall              # removes everything except data
garagetytus uninstall --keep-data  # preserves data dir for re-install
```

What `uninstall` does:

1. Best-effort `stop` (tolerates "already stopped" exit codes).
2. Removes the launchd plist (Mac) or systemd unit (Linux).
3. Deletes the `s3-service` keychain entry.
4. Deletes config + logs directories.
5. Deletes data directory **unless** `--keep-data` is passed.

Idempotent — second invocation is a no-op (everything already
gone). Per-grant keychain entries are deleted alongside their
grants on `revoke` and don't need separate cleanup.

After uninstall, no garagetytus state survives on disk except the
data dir (if `--keep-data` was used). The Garage binary itself
stays — `brew uninstall garage` (Mac) or `rm ~/.local/bin/garage`
(Linux) if you also want it gone.

---

## 15. Versioning, upgrades, AGPL posture

### SemVer

`v0.1.x` patches are bug-fix only; the JSON shapes
(grants store, `/metrics`, `bucket list --json`) are stable
across the `v0.1` line. `v0.2` introduces Windows + may bump
schema version (a migration runs on first start; rollback
unsupported).

### Upgrade path

```bash
brew upgrade traylinx/tap/garagetytus     # Mac
curl -fsSL garagetytus.dev/install | sh   # Linux re-runs installer
garagetytus restart                        # picks up new binary
```

The watchdog `keychain-migrate` step also handles legacy
`(makakoo, makakoo-s3-service)` → `(garagetytus, s3-service)`
keychain rotation on first start after upgrade.

### AGPL posture (LD#1)

Garage is AGPL-3.0-or-later. garagetytus stays MIT by maintaining
a hard subprocess fence — no `garage_*` Rust crate appears in
`Cargo.lock`, no `extern crate garage_*` anywhere in source.
Three CI jobs enforce this on every PR:

1. **Contract test** — greps `Cargo.lock` for any `garage-*`
   pattern, hard-fails if found.
2. **AGPL grep** — recursive grep for `use garage_*` and
   `extern crate garage_*` in `crates/`.
3. **`cargo-deny`** — bans every `garage_*` crate at the
   resolver level + permissive-license allowlist.

`THIRD_PARTY_NOTICES` ships the Garage attribution + AGPL
upstream source URL + the pinned tarball SHA. `garagetytus
about` surfaces the same values for inspection.

You can build proprietary apps on top of garagetytus without
inheriting the AGPL viral clause — the AGPL boundary is at the
subprocess fence, not the API.

---

## See also

- `docs/install/{macos,linux,windows}.md` — per-OS install notes.
- `docs/usage/quickstart.md` — five-minute quickstart.
- `docs/usage/grants.md` — grant grammar reference.
- `docs/integrate/{makakoo,tytus,external-app}.md` — integration
  contracts.
- `verdicts/Q1-VERDICT.md`, `Q2-VERDICT.md`, `Q3-AC8-RECOVERY.md`
  — locked design decisions (lope-in-the-loop pi+codex rounds).
- `THIRD_PARTY_NOTICES` — Garage attribution + license.
- `CHANGELOG.md` — release history.

For agent-driven workflows (Claude / Gemini / Codex / pi / etc.),
see the four SKILL.md files in `skills/garagetytus-{install,
bootstrap,daily-ops,troubleshoot}/` — each carries a decision
tree that an AI agent can read and execute autonomously.
