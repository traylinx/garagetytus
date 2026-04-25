---
name: garagetytus-daily-ops
version: 0.1.0
description: |
  Day-to-day operations on a bootstrapped garagetytus daemon —
  start / stop / restart / status, bucket lifecycle, grant
  lifecycle (mint / revoke / expire / deny-all), reading metrics
  + watchdog state, and recovery from unclean shutdown. The
  catch-all for "the daemon is up, now what."
allowed-tools:
  - Bash
  - Read
category: infrastructure
tags:
  - garagetytus
  - lifecycle
  - buckets
  - grants
  - metrics
  - recovery
---

# garagetytus-daily-ops — bucket + grant + lifecycle workflows

Use this skill for any operational task on a daemon that's already
installed and bootstrapped. It covers the four common workflows
the user is likely asking about: lifecycle (start/stop/restart/
status), bucket lifecycle (create/list/info/expire), grant
lifecycle (grant/revoke/deny-all), and observability (metrics,
watchdog state, unclean shutdown).

## When to reach for it

- "Start / stop / restart garagetytus" — lifecycle.
- "Create a bucket called X" / "list buckets" / "delete bucket Y"
  — bucket lifecycle.
- "Mint S3 creds for app Z" / "revoke that grant" / "lock down
  bucket Q immediately" — grant lifecycle.
- "Is the daemon healthy?" / "what's the disk free percentage?"
  / "did we have an unclean shutdown?" — observability.

## When NOT to reach for it

- The daemon isn't installed yet — `garagetytus-install`.
- The daemon is installed but not bootstrapped — `garagetytus-bootstrap`.
- A specific symptom needs diagnosis — `garagetytus-troubleshoot`.
- You need to integrate with boto3 / rclone / pandas — manual
  §12 has the integration shapes; this skill stops at the
  `garagetytus bucket grant ... --json` envelope handoff.

## Workflow A — Lifecycle

```bash
garagetytus start         # bootstrap launchd / systemctl
garagetytus stop          # bootout / stop unit
garagetytus restart       # stop + start
garagetytus status        # current state (running pid + state)
garagetytus serve         # foreground; Ctrl-C to stop
```

**Notes for agents driving these:**

- `start` is idempotent — already-running prints success.
- `stop` is idempotent — already-stopped prints success.
- `restart` is `stop` + `start` so its time-cost is ~1–3 s on Mac,
  ~1 s on Linux.
- `serve` blocks the terminal. Don't reach for it in a
  non-interactive agent context unless you're orchestrating
  multiple daemons.

If lifecycle operations fail with `port collision` or `daemon not
responding`, hand off to `garagetytus-troubleshoot`.

## Workflow B — Bucket lifecycle

### Create

```bash
garagetytus bucket create <name> --ttl <ttl> --quota <quota>
```

| Flag | Grammar | Required |
|---|---|---|
| `--ttl` | `30m / 1h / 24h / 7d / permanent` | yes |
| `--quota` | `100M / 1G / 10G / unlimited` | yes |

`--quota unlimited` requires `--confirm-yes-really` — do **not**
silently add the confirm flag for the user; ask first.

### List + inspect

```bash
garagetytus bucket list                  # human table
garagetytus bucket list --json           # machine-readable
garagetytus bucket info <name>           # ttl, quota, grants, usage
```

### Expire / delete

```bash
garagetytus bucket expire <name>         # for TTL'd buckets
garagetytus bucket deny-all <name> --ttl 1h    # emergency stop
```

`expire` runs `deny-all` internally before deleting the underlying
bucket so no presigned URL races the delete. Use `deny-all`
manually only when you suspect an in-flight leak.

## Workflow C — Grant lifecycle

### Mint

```bash
garagetytus bucket grant <bucket> \
    --to "<app-name>" \
    --perms read,write \
    --ttl 1h \
    --json
```

`--perms` is comma-separated from `read|write|list|delete`. `--ttl`
uses the same grammar as bucket creation (no `permanent` allowed
for grants — too risky).

Output envelope:

```json
{
  "grant_id": "g_<date>_<8-hex>",
  "access_key": "GK...",
  "secret_key": "...",
  "endpoint_url": "http://127.0.0.1:3900",
  "expires_at": "<iso8601>"
}
```

Hand the four values to the user. Never echo `secret_key` to a
shared log channel — treat it like a real AWS secret.

### Revoke

```bash
garagetytus bucket revoke <grant_id>
```

Atomic three-state transition (`active → revoking → revoked`). If
the backend key-delete fails, the watchdog retries every 60 s
until clean. The CLI returns success after the state transition
is logged, even if the backend retry is still in flight — that's
intentional; the operator already gets the audit-trail receipt.

### Deny-all (emergency stop)

```bash
garagetytus bucket deny-all <bucket> --ttl 1h
```

Flips a Garage flag that 403s every read/write to the bucket
including those carrying still-valid presigned URLs. Use when:
- A grant secret leaked.
- A bucket needs immediate freeze before forensic work.
- An automated pipeline is misbehaving and you can't reach the
  client to revoke its grant.

`deny-all` is reversible — `garagetytus bucket allow <bucket>`
flips the flag back. The TTL on `deny-all` is a safety belt; if
you forget to allow back, the bucket auto-unlocks after the TTL
expires.

## Workflow D — Observability

```bash
# Live metrics (Prometheus text):
curl -s http://127.0.0.1:3904/metrics

# JSON mirror (CLI / dashboards):
cat <data_dir>/watchdog.json    # paths in manual §13
```

Five gauges/counters exposed (LD#11):

| Metric | Read as |
|---|---|
| `garagetytus_disk_free_pct` | Float 0–100. <10 → mode flips to ro. >15 → mode back to rw (hysteresis). |
| `garagetytus_mode{mode="rw"\|"ro"}` | Exactly one is `1`, the other absent. |
| `garagetytus_uptime_seconds` | Since `serve` started. Restart → resets to 0. |
| `garagetytus_unclean_shutdown_total` | Monotonic. Increments per kill-9 / OOM / panic event. |
| `garagetytus_watchdog_last_tick_unix_seconds` | Should be within ~30s of `time.time()`. Stale → daemon hung. |

## Workflow E — Unclean shutdown recovery (AC8)

Symptom: the daemon was kill-9'd, OOM-killed, or the laptop lost
power. On the next `garagetytus serve`:

1. The preflight detects the orphan PID in `sentinel.lock`.
2. `garagetytus_unclean_shutdown_total` increments by 1 (persisted
   to `unclean_shutdown_total.txt` so it survives restarts).
3. Auto-repair fires post-spawn iff the cluster is single-node
   (the v0.1 default): shells `garage -c <cfg> repair tables --yes`
   with a 15 s health-check budget.
4. Repair never blocks startup — failures are logged via
   `tracing::warn!` and swallowed.

To verify auto-repair ran after a known-unclean restart:

```bash
journalctl --user -u garagetytus.service | grep -E "(unclean|repair tables)"
# or for foreground serve:
grep -E "(unclean|repair tables)" <log_dir>/garagetytus.log
```

Manual recovery (rarely needed — `repair tables` already runs
auto on single-node):

```bash
# After daemon is up:
garage -c <config-path> repair tables --yes        # safe, sub-second
garage -c <config-path> repair start --all --yes   # heavy, hours; multi-node only
```

## Failure mode handling

If any workflow above produces an unexpected error, hand off to
`garagetytus-troubleshoot` with the symptom and exit code. Don't
guess at fixes — the troubleshoot skill has the full symptom →
cause → fix matrix.

## What this skill does NOT do

- **Bootstrap a fresh daemon.** Use `garagetytus-bootstrap`.
- **Install on a fresh host.** Use `garagetytus-install`.
- **Diagnose symptoms.** Use `garagetytus-troubleshoot`.
- **Configure non-default ports / keychains / paths.** That's a
  manual `garagetytus.toml` edit; document the change but don't
  perform it without explicit user direction.
- **Wire boto3 / rclone / pandas.** Manual §12 has the recipes;
  this skill ends at the grant JSON envelope.

## See also

- Manual: `docs/MANUAL.md` §7 (lifecycle), §8 (buckets+grants),
  §9 (observability), §10 (recovery).
- `verdicts/Q3-AC8-RECOVERY.md` — auto-repair design rationale.
- `garagetytus-troubleshoot` — when something goes wrong.
