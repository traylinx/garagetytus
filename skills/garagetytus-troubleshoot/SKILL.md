---
name: garagetytus-troubleshoot
version: 0.1.0
description: |
  Diagnose + fix problems with a garagetytus install. Symptom-
  driven decision tree covering the install path, bootstrap path,
  daemon lifecycle, port collisions, keychain issues, integration
  failures (boto3 403/400, rclone), and observability gaps. Always
  reach for this skill before suggesting a reinstall.
allowed-tools:
  - Bash
  - Read
category: infrastructure
tags:
  - garagetytus
  - troubleshoot
  - debug
  - support
---

# garagetytus-troubleshoot — symptom → cause → fix

Use this skill the moment a user reports a garagetytus problem
that doesn't immediately match an install / bootstrap / daily-ops
workflow. The matrix below is the first lookup; if the symptom
isn't listed, fall through to the §"Information to gather" block
and ask the user for it before guessing.

## When to reach for it

- Any error message during install, bootstrap, lifecycle, or
  bucket/grant operations.
- "It worked yesterday and now it doesn't."
- `/metrics` returns 503 or unexpected values.
- A boto3 / rclone / pandas client gets 4xx/5xx errors.
- The daemon is "running" by `status` but unresponsive.

## When NOT to reach for it

- The user is on a fresh host with no install yet — that's
  `garagetytus-install`, not a troubleshoot scenario.
- The user wants to do a normal operation (create bucket, mint
  grant) and hasn't reported any failure — `garagetytus-daily-ops`.

## Information to gather first

When a user reports a vague problem ("it's broken"), ask for
these three before diagnosing:

```bash
garagetytus about              # version + Garage pin
garagetytus status             # running / stopped / error
curl -s http://127.0.0.1:3904/metrics | head -20
# plus the last 50 lines of the log:
tail -50 ~/Library/Logs/garagetytus/garagetytus.log         # Mac
tail -50 ~/.local/share/garagetytus/logs/garagetytus.log    # Linux
```

For systemd-managed daemons:

```bash
journalctl --user -u garagetytus.service -n 100 --no-pager
```

For launchd-managed:

```bash
launchctl print gui/$(id -u)/com.traylinx.garagetytus | head -40
```

## Symptom matrix

### Install path

| Symptom | Cause | Fix |
|---|---|---|
| `brew: command not found` (Mac) | Homebrew not installed | Tell user to install at `https://brew.sh`. **Stop** — needs sudo. |
| `Error: traylinx/tap not found` | Tap not added | `brew tap traylinx/tap` then retry the install. |
| `garage: command not found` after `brew install garagetytus` | Formula didn't pull garage | `brew install garage` directly, then `garagetytus install`. |
| Linux SHA mismatch on `garagetytus install` | Pinned hash drifted from upstream tarball | **Refuse to bypass.** File an issue with the SHA the installer expected vs. the SHA it got. |
| `garagetytus install` hangs on Linux | Network slow downloading garage musl binary | Wait — first install pulls ~30 MB. If >5 min, kill + retry on a different network. |
| `Permission denied` writing to `~/.local/bin/garage` | Path exists with wrong owner | `ls -la ~/.local/bin/garage`; if owned by root from a prior sudo install, `sudo rm` it then retry. |

### Bootstrap path

| Symptom | Cause | Fix |
|---|---|---|
| `daemon not responding at <admin-url>` | `garagetytus start` didn't start the daemon | Check `garagetytus status`; if stopped, inspect launchd / systemd logs. |
| `garage key create failed (exit N)` | Garage state stale or layout missing | `garagetytus uninstall --keep-data` then re-install + bootstrap. Data dir survives. |
| `could not parse access_key + secret_key from garage CLI output` | Upstream Garage CLI output shape changed | File an issue with the verbatim output. Don't monkey-patch. |
| Mac keychain prompts for password during bootstrap | Keychain locked | Unlock keychain (`security unlock-keychain`) then retry bootstrap. |

### Daemon lifecycle

| Symptom | Cause | Fix |
|---|---|---|
| `port collision — refusing to start` | One of 3900/3901/3903/3904 bound | The error names the port + offending PID hint. Either kill the offender or remap in `garagetytus.toml`. |
| Mac launchd plist won't load (`bootstrap` exit 5) | Old plist format from prior version | `launchctl bootout gui/$(id -u)/com.traylinx.garagetytus 2>/dev/null; rm ~/Library/LaunchAgents/com.traylinx.garagetytus.plist; garagetytus install`. |
| systemd unit fails to start | User-mode services disabled at session level | `loginctl enable-linger $USER && systemctl --user daemon-reload && systemctl --user start garagetytus`. |
| `garagetytus status` says running but `/metrics` is 503 | Daemon up but watchdog hasn't ticked yet | Wait 30 s. If still 503 after 60 s, daemon is hung — `garagetytus restart`. |
| Daemon exits immediately after start | Config broken (token mismatch, bad TOML) | `tail` the log; common cause is a hand-edited `garagetytus.toml` with a quote/bracket error. |
| `garagetytus_unclean_shutdown_total` keeps incrementing without unclean events | Watchdog can't write `sentinel.lock` (disk full / permissions) | Check `garagetytus_disk_free_pct`; check `<data_dir>/sentinel.lock` is writable. |

### Bucket / grant operations

| Symptom | Cause | Fix |
|---|---|---|
| `service keypair missing in keychain` on `bucket list` | Bootstrap never ran, or ran in a different keychain session | Hand off to `garagetytus-bootstrap`. On Mac, ensure same login session as the bootstrap shell. |
| `bucket create` errors with "name already taken" | Bucket exists already | `garagetytus bucket info <name>` to inspect, or pick a different name. |
| `bucket grant` errors with "rate limit: N active grants" | Hit the global active-grant cap (20) | `bucket list-grants` and revoke unused ones, or wait for TTLs to expire. |
| `bucket grant` errors with "rate limit: 50 create-ops/hour" | Burst protection tripped | Wait — limit is per-hour rolling. |
| `bucket revoke` succeeds but boto3 still works | boto3 cached creds OR backend key-delete still in flight | The watchdog retries every 60 s; check `bucket info` for grant state. boto3 cache: recreate the client. |

### S3 client integration

| Symptom | Cause | Fix |
|---|---|---|
| boto3 `400 Bad Request` | Default virtual-host addressing | `Config(s3={"addressing_style": "path"})`. Mandatory (LD#4). |
| boto3 `403 Forbidden` on PUT | Grant lacks `write` perm | Re-mint with `--perms read,write`. |
| boto3 `403 Forbidden` on every request | Bucket has `deny-all` flipped, or grant revoked | `garagetytus bucket info <name>` to check; `bucket allow <name>` if deny-all was the cause. |
| boto3 `404 NoSuchBucket` | Bucket TTL expired and watchdog cleaned it | Recreate the bucket; mint a fresh grant. |
| rclone hangs on `bisync` | Path-style addressing not configured | Add `force_path_style = true` to the rclone config block. |
| rclone fails with `SignatureDoesNotMatch` | Clock skew >5 min vs. host | Sync the system clock (`sntp -sS time.apple.com` Mac, `chronyc makestep` Linux). |

### Observability gaps

| Symptom | Cause | Fix |
|---|---|---|
| `/metrics` returns 503 | `watchdog.json` missing | Wait one tick (~30 s). If persistent, `garagetytus restart`. |
| `garagetytus_mode{mode="ro"} 1` | Disk free <10% — hysteresis flipped to read-only | Free disk space; mode flips back at 15%. Monitor `disk_free_pct`. |
| `watchdog_last_tick` stale by >2 min | Watchdog thread crashed | `garagetytus restart`. File an issue with the log. |
| `unclean_shutdown_total` jumps by >1 per known event | Race in counter persistence (rare) | File issue. Counter is best-effort, not load-bearing. |

## When the matrix doesn't help

If the symptom isn't on the matrix and the gathered information
doesn't suggest an obvious fix:

1. **Don't guess at a reinstall.** Reinstall is the nuclear option;
   it rebuilds keychain entries and risks losing per-grant state.
2. **Capture more data.** Re-run with `RUST_LOG=garagetytus=debug,
   reqwest=info` and replicate the failure.
3. **File an issue** at the garagetytus repo with: `about` output,
   the failing command, full stderr, and the relevant log slice.
4. As a last resort, `garagetytus uninstall --keep-data` + reinstall
   + re-bootstrap. Data dir survives; grants do not (they're in
   `grants.json` which is in the config dir, **not** kept by
   `--keep-data`).

## Recovery from a half-broken state

If a previous session left the host in an inconsistent state
(partially-written config, orphan plist, stale keychain entry):

```bash
garagetytus uninstall --keep-data    # removes config + service unit + keychain
# then verify nothing remains:
ls ~/Library/LaunchAgents/ | grep garagetytus    # Mac — should be empty
ls ~/.config/systemd/user/ | grep garagetytus    # Linux — should be empty
security find-generic-password -s garagetytus -a s3-service 2>/dev/null    # Mac — should be empty
# then reinstall:
garagetytus install
garagetytus start
garagetytus bootstrap
```

The data dir survives `--keep-data`. Cluster layout and bucket
data persist; you'll need to re-mint grants because `grants.json`
lives in the config dir which gets wiped.

## What this skill does NOT do

- **Modify `garagetytus.toml` automatically.** Token + port edits
  are the user's call; this skill diagnoses, doesn't reconfigure.
- **Bypass the SHA pin or AGPL contract.** Linux SHA mismatch is
  fatal by design.
- **Force-restart on a hung daemon without surfacing the cause.**
  Always inspect logs first; restart is a fix, not a diagnosis.
- **Touch the user's network or system clock.** Clock-skew fixes
  go through the OS (sntp / chronyc), not garagetytus.

## ⚠️ Hard rule — health-check semantics

**`HTTP 403 "Forbidden: Garage does not support anonymous access yet"`
is the *healthy* response.** Garage requires SigV4-signed requests
for every operation including `/health`. An anonymous probe gets
a structured 403 with that exact XML body — that proves the daemon
is up, listening, and responding. Do NOT report it as "down".

The same rule applies to the Tytus shared service at
`https://garagetytus.traylinx.com` — the public Caddy endpoint
proxies the same anonymous-deny response through unchanged.

Real outage signatures:

- `curl --max-time 5` exits 28 (timeout) or 7 (connection refused).
- HTTP `502 Bad Gateway` from Caddy (Garage daemon dead behind it).
- boto3 `EndpointConnectionError` / `ConnectTimeoutError`.

Symptoms that are NOT outages:

- HTTP 403 with Garage `AccessDenied` XML body (= healthy).
- `SignatureDoesNotMatch` (= clock skew or wrong key).
- `NoSuchBucket` (= grant missing or wrong name).
- Empty bucket list (= bucket has no objects, healthy).

When you DO suspect an outage, your report to the user MUST
include: (1) exact command run, (2) exit code, (3) verbatim
stderr first 500 chars, (4) ISO 8601 UTC timestamp. Reports
without these four fields are inadmissible — say "I don't know
the status; I haven't probed it" instead of guessing. This rule
lives in `MAKAKOO/bootstrap/global.md` and rides every CLI host.

## See also

- Manual: `docs/MANUAL.md` §11 (full troubleshooting matrix),
  §16 (public HTTPS endpoint via Caddy), §17 (health-check semantics).
- `garagetytus-install`, `garagetytus-bootstrap`,
  `garagetytus-daily-ops` — the workflows this skill supports.
- `verdicts/Q3-AC8-RECOVERY.md` — what auto-repair does and
  doesn't do on unclean shutdown.
