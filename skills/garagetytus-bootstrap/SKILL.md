---
name: garagetytus-bootstrap
version: 0.1.0
description: |
  First-run bootstrap of an installed garagetytus daemon — start
  the service, run admin-API layout assignment, provision the
  `s3-service` keypair, and verify the daemon is healthy. Idempotent
  end-to-end. Use after `garagetytus install` and before any
  bucket / grant work.
allowed-tools:
  - Bash
category: infrastructure
tags:
  - garagetytus
  - bootstrap
  - first-run
  - admin-api
  - keychain
---

# garagetytus-bootstrap — first-run setup workflow

Use this skill after `garagetytus install` has succeeded but
before any bucket or grant operations. It walks the user from
"installed but inert" to "daemon healthy, layout assigned, service
keypair in keychain, ready to issue grants."

## When to reach for it

- Right after `garagetytus install` on a fresh host.
- After a clean reinstall (uninstall + install) to re-seed the
  service keypair.
- When `garagetytus status` reports running but `bucket list`
  errors with "service keypair missing in keychain."

## When NOT to reach for it

- The daemon isn't installed yet — use `garagetytus-install` first.
- The daemon is already bootstrapped and you want to mint a grant
  — use `garagetytus-daily-ops`.
- Bootstrap previously succeeded and you're seeing a non-keypair
  symptom — diagnose via `garagetytus-troubleshoot`.

## Decision tree

### Step 1 — Confirm install state

```bash
garagetytus about              # must print without error
garagetytus status             # may say running or stopped
```

If `garagetytus about` errors → daemon binary is missing or broken.
**Stop.** Hand off to `garagetytus-install`.

### Step 2 — Bring the service up (idempotent)

```bash
garagetytus start
```

Tolerate two known exit codes:
- `0` — newly started.
- already-running message printed → that's success too.

If `garagetytus start` fails with `port collision`, hand off to
`garagetytus-troubleshoot` (port-collision symptom). Don't proceed.

Wait for the admin API to come up (1–2 s typically):

```bash
sleep 2
```

### Step 3 — Run bootstrap

```bash
garagetytus bootstrap
```

Expected output (one block per step):

```
garagetytus bootstrap: using admin API at http://127.0.0.1:3903
  daemon health: ok
  layout: assigned (zone=local, capacity=1)
  service keypair: created + stored in keychain (account=s3-service)
garagetytus bootstrap: done. Try `garagetytus bucket create demo --ttl 1h`.
```

The bootstrap is idempotent — re-running on an already-bootstrapped
host prints:

```
  layout: already assigned
  service keypair: already in keychain (service=garagetytus, account=s3-service)
```

Both shapes are success. Don't try to "fix" the second one.

### Step 4 — Verify health end-to-end

```bash
curl -s http://127.0.0.1:3904/metrics | grep -E "^garagetytus_(mode|disk_free_pct|uptime_seconds)"
```

Expected:

```
garagetytus_mode{mode="rw"} 1
garagetytus_disk_free_pct <value>
garagetytus_uptime_seconds <value>
```

If `mode="ro"` is set, the data partition is below 10% free —
warn the user before they create buckets; writes will be rejected.

If `/metrics` returns **HTTP 503**, the watchdog hasn't ticked yet
(daemon just started). Wait 30 s and re-check. If still 503 after
60 s, hand off to `garagetytus-troubleshoot`.

### Step 5 — Hand off

Tell the user the daemon is **bootstrapped and ready to issue
grants**. Suggest the obvious next move:

```bash
garagetytus bucket create my-data --ttl 7d --quota 1G
garagetytus bucket grant my-data --to "<your-app>" --perms read,write --ttl 1h --json
```

Or hand off to `garagetytus-daily-ops` for the full lifecycle
walkthrough.

## Failure mode handling

| Symptom | Cause | Fix |
|---|---|---|
| `daemon not responding at <admin-url>` | `garagetytus start` didn't actually start the daemon | Inspect `garagetytus status`; check launchd / systemd logs; hand off to `garagetytus-troubleshoot`. |
| `garage key create failed (exit N)` | Garage config tokens stale (host previously had a partial install) | `garagetytus uninstall --keep-data` then re-run install + bootstrap. Data dir survives. |
| `could not parse access_key + secret_key from garage CLI output` | Upstream Garage version drift | File an issue with the garage CLI output verbatim. Don't try to monkey-patch the parser. |
| `keychain locked` / Mac prompts for password | Keychain not unlocked at bootstrap time | Tell user to unlock the keychain (or log in to a fresh GUI session); retry. |
| Bootstrap succeeds but `bucket list` says "no service keypair" | Keychain wrote to the wrong session | macOS: the session that ran bootstrap and the session running `bucket list` must share the same login keychain. Re-run bootstrap from the same shell as `bucket list`. |

## What `bootstrap` writes to the system

After successful bootstrap:

| Location | Content |
|---|---|
| `<data_dir>/meta/`, `<data_dir>/data/` | Garage cluster state (sled DB, layout) |
| Keychain: `(garagetytus, s3-service)` | JSON: `{access_key, secret_key, endpoint}` |
| `<config_dir>/grants.json` | Created empty (LD#9 — read-only by Makakoo + tytus) |

No new files in `$HOME` outside the per-OS app dirs. No
environment variables touched.

## Idempotence guarantee

Every command in this skill is safe to re-run. `start` on a
running daemon is a no-op. `bootstrap` on a bootstrapped host
detects the existing layout + keypair and prints "already" lines.
A failed bootstrap + retry cycle leaves no inconsistent state.

## See also

- Manual: `docs/MANUAL.md` §6 (first-run bootstrap) + §13
  (config reference).
- `garagetytus-install` — the prerequisite before this skill.
- `garagetytus-daily-ops` — the natural next step after this skill.
- `verdicts/Q3-AC8-RECOVERY.md` — what happens if the daemon
  crashes mid-bootstrap.
