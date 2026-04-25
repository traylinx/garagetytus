# garagetytus-grants

Runtime user-grants store, carved from Makakoo v0.7.1
(`makakoo-core::capability::user_grants`) on 2026-04-25 as part of
GARAGETYTUS-V0.1 Phase A.5.

## Schema authority

`~/.garagetytus/grants.json` (Mac/Linux) or
`%APPDATA%\garagetytus\grants.json` (Windows). Schema version: 1.

`garagetytus` is the **sole writer**. Makakoo and tytus are read-only
consumers via the same on-disk file (LD#9 — see SPRINT.md).

## Design notes (frozen at carve-out)

- Sidecar lock at `<path>.lock`, NEVER on the data fd; released after
  `fs::rename` completes (LD#9).
- Machine-local, gitignored, never synced (LD#4).
- No `use_count` / `last_used_at` on the schema, no `record_use()`
  method — audit log answers "was this grant used" (Lope F4).
- `origin_turn_id` stored but not enforcement-bound until v0.3.1
  (Lope F6).
- Rate-limit counter lives in a separate file
  (`state/perms_rate_limit.json`) so a corrupt counter cannot poison
  the grants (Lope F7).

## Public API

```rust
use garagetytus_grants::{UserGrant, UserGrants, default_path, new_grant_id};
```

See `src/lib.rs` for the full surface.

## License

MIT — see workspace `Cargo.toml`. The Garage S3 daemon that
garagetytus orchestrates is AGPL-3.0-or-later (subprocess only,
never linked — LD#1).
