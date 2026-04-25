# Using garagetytus with Makakoo

> **Audience:** you have an existing [Makakoo](https://github.com/makakoo/makakoo-os)
> install (the agentic Rust workspace) and want to know how
> garagetytus fits in.
>
> **TL;DR — zero migration cost.** Install garagetytus, run
> `garagetytus install + start + bootstrap`, keep using
> `makakoo bucket *` exactly like before. No code change in any
> agent that consumed Makakoo's S3 surface.

## What changed underneath

Pre-v0.1, Makakoo bundled a Garage daemon and owned ~1300 LOC of
bucket lifecycle in `makakoo-os/makakoo/src/commands/bucket.rs`.
Per the GARAGETYTUS-V0.1 sprint (Q2 lope verdict, pi+codex PASS
Option A), that logic now lives **only** in this repo at
`crates/garagetytus/src/commands/bucket.rs`. Makakoo's
`bucket` subcommand became a ~250-LOC wrapper that exec's
`garagetytus bucket *` with inherited stdio.

You don't notice. The CLI surface is identical:

```bash
makakoo bucket create my-data --ttl 7d --quota 1G
makakoo bucket grant my-data --to my-app --perms read,write --ttl 1h --json
makakoo bucket list --json
makakoo bucket revoke g_20260425_a1b2c3d4
```

Each invocation forks once, exec's `garagetytus bucket <args>`,
and inherits stdin/stdout/stderr verbatim. The `--json` envelope
that comes back is the exact JSON garagetytus emits — no
re-wrapping, no schema drift.

## Install order (one-time, then forget)

1. Whatever you do today to install Makakoo (unchanged — Makakoo
   no longer ships Garage).
2. Install garagetytus:
   ```bash
   curl -fsSL --proto '=https' --tlsv1.2 \
     https://raw.githubusercontent.com/traylinx/garagetytus/main/install/install.sh | bash
   ```
3. Bring the daemon up:
   ```bash
   garagetytus install
   garagetytus start
   garagetytus bootstrap
   ```
4. Verify Makakoo sees it:
   ```bash
   makakoo bucket list      # should print "(no buckets registered)" without error
   ```

That's it. From this point `makakoo bucket *` is a thin shim, and
you can use either the `makakoo` or `garagetytus` flavor of the
subcommands interchangeably — same daemon, same store, same JSON.

## What stays Makakoo-side (no change)

| Surface | Where | Behavior |
|---|---|---|
| `makakoo bucket *` CLI | `makakoo-os/makakoo/src/commands/bucket.rs` | thin wrapper, exec's garagetytus |
| `core.s3.client()` Python helper | `MAKAKOO/plugins/lib-harvey-core/src/core/s3/__init__.py` | reads service identity from OS keychain → returns boto3 client wired to `127.0.0.1:3900` with path-style addressing locked (LD#14) |
| Capability shims | `makakoo-os/makakoo-core/src/capability/{user_grants,rate_limit,audit,audit_escape}.rs` | 4-line files each: `pub use garagetytus_grants::*;` |
| Audit log + rate-limit + grants store | `garagetytus_grants` crate (linked into Makakoo) | schema version 1, frozen across both repos |
| `plugins-core/garage-store/` plugin | `makakoo-os/plugins-core/garage-store/` | service-bridge adapter (`kind = "service-bridge"` in `adapter.toml`); `bin/install.sh` + `bin/garage-wrapper.sh` 100% defer to garagetytus |

## What's gone (intentionally)

- The bundled Garage acquisition + daemon lifecycle code that
  used to live inside Makakoo. It's now garagetytus's exclusive
  job. Single source of truth (LD#12 — no tytus-aware or
  Makakoo-aware code in garagetytus, no garagetytus-internal
  duplication elsewhere).
- The Phase D.3 "fallback to a Makakoo-bundled Garage if
  garagetytus isn't installed" branch. Doesn't exist.
  `adapter.toml` declares `embedded = false`. If the daemon
  isn't on PATH, `makakoo bucket` prints
  `garagetytus not found — install at https://garagetytus.dev`
  to stderr and exits non-zero. No silent degraded mode.

## How your existing agents see it

Three layers, all unchanged:

### 1. Python agents using `core.s3.client()`

```python
from core.s3 import client

s3 = client()                                        # service identity
s3 = client(grant_id="g_20260425_a1b2c3d4")          # per-grant scope
s3.put_object(Bucket="my-data", Key="hello.txt", Body=b"hi")
```

Same import, same call shape. The keychain entry the helper
reads is `(service="garagetytus", account="s3-service")` after
v0.1 (was `(service="makakoo", account="makakoo-s3-service")`
before). The watchdog's keychain-migrate step copies the legacy
entry on first daemon start after upgrade — your agents don't
notice.

When the standalone `garagetytus-sdk` PyPI package publishes
(gated on the v0.1.0 non-rc tag), `core.s3` becomes a thin
re-export shim of `garagetytus`. Same `from core.s3 import
client` import, new package providing the implementation. Phase
C.3 of the sprint.

### 2. Rust code consuming the grants store

Anywhere in `makakoo-os` that imports
`makakoo_core::capability::user_grants::*` already resolves to
`garagetytus_grants::*` at link time. The 4-line shim files in
`capability/` make this transparent. No Cargo.toml change, no
import rewrite.

If you're writing a NEW Rust crate that wants to read the
grants store directly, prefer the canonical name:

```toml
# Cargo.toml
garagetytus-grants = { git = "https://github.com/traylinx/garagetytus", branch = "main" }
```

```rust
use garagetytus_grants::{UserGrants, UserGrant};

let grants = UserGrants::load_at(&garagetytus_core::paths::grants_path())?;
for g in grants.active_for_bucket("my-data") { ... }
```

The shim re-export from `makakoo_core::capability::*` keeps
working forever for Makakoo-internal code.

### 3. The `garage-store` plugin

`adapter.toml` declares the discovery contract:

```toml
[adapter]
name = "garagetytus"
kind = "service-bridge"
endpoint = "http://127.0.0.1:3900"
admin_endpoint = "http://127.0.0.1:3903"
binary = "garagetytus"
keychain_account = "s3-service"
keychain_service = "garagetytus"

[discovery]
probe = "garagetytus capabilities --json"
required_version = "0.1"

[install_hint]
darwin = "brew install traylinx/tap/garagetytus"
linux = "curl -fsSL garagetytus.dev/install | sh"

[fallback]
embedded = false
message = "garagetytus not found — install at https://garagetytus.dev"
```

`makakoo plugin status garage-store` health-checks the daemon by
hitting `http://127.0.0.1:3903/health`. The plugin's
`restart_policy = "never"` (in `plugin.toml`) prevents
double-actor collisions — garagetytus owns the process
lifecycle, period.

## The AGPL boundary

Garage upstream is AGPL-3.0-or-later. Makakoo stays MIT by:

1. Never linking any `garage_*` Rust crate. Enforced by a
   contract test at
   `makakoo-os/makakoo/tests/contract_no_garage_crates.rs` that
   greps Cargo.lock and hard-fails CI on any match.
2. Invoking `garage` only as a subprocess child of garagetytus.
   The subprocess fence is the legal boundary.
3. The `garage-store` plugin manifest grants only
   `exec/binary:.../garagetytus` — no permission to load Garage
   as a library.

Your code on top of Makakoo + garagetytus inherits **MIT terms**.
Garage's AGPL clauses don't propagate across the subprocess
fence.

## CLI subcommand parity

Every `makakoo bucket <sub>` exists 1:1 in garagetytus:

| `makakoo bucket ...` | `garagetytus bucket ...` |
|---|---|
| `create <name> [--ttl] [--quota]` | `create <name> [--ttl] [--quota]` |
| `list [--json]` | `list [--json]` |
| `info <name> [--json]` | `info <name> [--json]` |
| `grant <bucket> --to <label> --perms <p> --ttl <t> [--json]` | `grant <bucket> --to <label> --perms <p> --ttl <t> [--json]` |
| `revoke <grant-id>` | `revoke <grant-id>` |
| `expire [--dry-run]` | `expire [--dry-run]` |
| `deny-all <name> --ttl <t> [--confirm-yes-really]` | `deny-all <name> --ttl <t> [--confirm-yes-really]` |

You can mix freely — `makakoo bucket grant ... --json` and
`garagetytus bucket grant ... --json` produce identical
envelopes and write to the same `grants.json` store.

## Source pointers (for code archaeology)

| File | Purpose |
|---|---|
| `makakoo-os/makakoo/src/commands/bucket.rs` (commit `ae97464`) | Wrapper — `spawn_garagetytus()` at line 41 |
| `makakoo-os/plugins-core/garage-store/adapter.toml` | Discovery + install hints + fallback policy |
| `makakoo-os/plugins-core/garage-store/bin/install.sh` | 100% deferral to `garagetytus install` |
| `makakoo-os/plugins-core/garage-store/bin/garage-wrapper.sh` | Lifecycle dispatch — `exec garagetytus {start,stop,status}` |
| `makakoo-os/plugins-core/garage-store/plugin.toml` | `restart_policy = "never"` + grant scope `exec/binary:...garagetytus` |
| `makakoo-os/makakoo-core/src/capability/{user_grants,rate_limit,audit,audit_escape}.rs` | 4-line `pub use garagetytus_grants::*;` shim files |
| `makakoo-os/makakoo/tests/contract_no_garage_crates.rs` | LD#1 contract test — no `garage-*` crate may be linked |
| `MAKAKOO/plugins/lib-harvey-core/src/core/s3/__init__.py` | Python `core.s3.client()` — pre-Phase-C.3 |

## Verdicts and reasoning

- **Q2 (Makakoo bucket wrapper):** pi + codex PASS Option A —
  thin wrapper preserves discoverability + restores single source
  of truth. `Stdio::inherit()` over capture-and-relay avoids
  schema drift on `--json` output. See
  `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/verdicts/Q2-VERDICT.md`.
- **LD#1 (AGPL boundary):** Garage is a child process, never
  linked. Three CI gates enforce.
- **LD#9 (grants store ownership):** garagetytus is the **sole
  writer**; Makakoo (and tytus) are **read-only consumers**.
  Path is `dirs::config_dir().join("garagetytus/grants.json")`.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `makakoo bucket <sub>` returns "garagetytus not found — install at https://garagetytus.dev" | Daemon binary not on PATH | Run the one-liner installer above. |
| `makakoo bucket list` works but `core.s3.client()` errors with "service keypair missing" | `garagetytus bootstrap` never ran, or Python venv reads a different keychain than the bootstrap shell | Re-run bootstrap from the shell that runs the Python agent. On Mac, ensure same login keychain session. |
| `harvey_describe_*` MCP tool returns 404 / no garage health | Garage subprocess crashed; garagetytus didn't restart yet | `garagetytus restart`; inspect `<log_dir>/garagetytus.log`. |
| `makakoo plugin status garage-store` says unhealthy but `garagetytus status` says running | Admin port mismatch (3903 vs override) | Check `garagetytus.toml` `[admin] api_bind_addr` matches `adapter.toml`'s `admin_endpoint`. |
| Old `(makakoo, makakoo-s3-service)` keychain entry still present after upgrade | Watchdog migrate step hasn't run yet (needs one full tick after restart) | Wait 30 s after `garagetytus start`, or run `security delete-generic-password -s makakoo -a makakoo-s3-service` manually on Mac. |

## Q&A

**Q: Do I need to update my Cargo.toml dependencies?**
A: No. The shim re-exports keep `use makakoo_core::capability::*`
working forever. New code can import `garagetytus_grants`
directly if you prefer the canonical name.

**Q: Can I run garagetytus and Makakoo's old bundled Garage at
the same time?**
A: There is no "old bundled Garage" anymore — Makakoo HEAD ships
zero garage spawn code. If you have a stale Makakoo install from
before the Q2 verdict, uninstall it and reinstall fresh.

**Q: What if I want to move my existing buckets/grants from a
pre-v0.1 Makakoo install?**
A: The data dir survives — point `garagetytus` at the same
`<config_dir>/garagetytus.toml` and `<data_dir>` and the existing
Garage state loads cleanly. Run the watchdog keychain-migrate
step (automatic on first start after upgrade) to copy the
service keypair from the old keychain account to the new one.

**Q: Does garagetytus replace tytus too?**
A: No — tytus is a separate product (private AI pods over
WireGuard). See [`tytus.md`](tytus.md) for how a tytus pod can
consume the mac-side garagetytus daemon.
