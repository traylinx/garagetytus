# Integrating with Makakoo

> **Status (2026-04-25):** ✅ implemented. Phase D of
> GARAGETYTUS-V0.1 ships the Makakoo-side wrapper +
> `plugins-core/garage-store/adapter.toml`. Source pointers
> below.

## What changed

Pre-v0.1, Makakoo's `makakoo bucket *` subcommand owned ~1300
LOC of bucket-lifecycle business logic in
`makakoo/src/commands/bucket.rs`. Post-v0.1, that logic lives
**only** in this repo
(`crates/garagetytus/src/commands/bucket.rs`); Makakoo's
`bucket` subcommand became a thin Rust wrapper that exec's
`garagetytus bucket *` with inherited stdio.

## Why a wrapper, not "delete the Makakoo subcommand"

Lope verdict (pi+codex 2026-04-25, both PASS Option A):

- Discoverability stays — `makakoo --help` keeps surfacing
  `bucket`. Users who learned the Makakoo brand don't have to
  learn a second binary's command tree.
- Single source of truth is restored: garagetytus is the only
  place bucket lifecycle is implemented.
- The Phase D.3 fallback contract holds cleanly: when
  garagetytus is absent, the wrapper prints
  `garagetytus not found — install at https://garagetytus.dev`
  to stderr and exits non-zero. **No silent embedded-Garage
  fallback.**

Full verdict:
`MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/verdicts/Q2-VERDICT.md`

## Source pointers

| File | Purpose |
|---|---|
| `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/SPRINT.md` | Sprint plan + lope verdicts |
| `makakoo-os/makakoo/src/commands/bucket.rs` (post-`ae97464`) | The wrapper (~250 LOC + 8 unit tests) |
| `makakoo-os/plugins-core/garage-store/adapter.toml` | Discovery probe + install hints + fallback message |
| `makakoo-os/makakoo-core/src/capability/{user_grants,rate_limit,audit,audit_escape}.rs` | Backward-compat shims that re-export from `garagetytus_grants` |
| `MAKAKOO/plugins/lib-harvey-core/src/core/s3/__init__.py` | Python SDK (currently unchanged; Phase C.3 flip lands post-PyPI publish) |

## What stays Makakoo-side

The MCP `harvey_bucket_*` tools in `harvey_mcp.py` keep their
`from core.s3 import client` import unchanged (Phase D.2 dogfood
contract from MAKAKOO-OS-V0.7.1-S3-MULTITENANCY). After the v0.1
PyPI publish + Phase C.3 flip, `core.s3` becomes a re-export shim
of `garagetytus`; the import still works, the source-grep AC12
test still passes.

## Install order for users

1. `makakoo install` (existing flow — does NOT install garage anymore).
2. `brew install traylinx/tap/garagetytus` (macOS) **or**
   `curl -fsSL garagetytus.dev/install | sh` (Linux).
3. `garagetytus install && garagetytus start && garagetytus bootstrap`.
4. `makakoo bucket create my-data` works (delegates to step 3's daemon).

The `plugins-core/garage-store/` Makakoo plugin is now an
adapter shim (kind="adapter", v0.2.0). Its
`bin/install.sh` defers to `garagetytus install`; its
`bin/garage-wrapper.sh` defers to `garagetytus {start,stop,
status}` via exec.
