# Integrating with tytus

> **Status (2026-04-25):** spec-only. Actual `tytus bucket`
> subcommand + tray-menu cell ship from the tytus codebase
> (separate private repo at github.com/traylinx/tytus) when the
> tytus team picks up Phase E. This doc is the **contract** they
> code against.

## What this is

[tytus](https://traylinx.com/tytus) is a private AI pod product —
a WireGuard-tunneled OpenAI-compatible LLM gateway running on
the user's account. tytus's binary lives at `/usr/local/bin/tytus`
(Mach-O on Mac, currently Mac-only).

Per GARAGETYTUS-V0.1 sprint Phase E, tytus adds two surfaces that
delegate to garagetytus:

1. A new `tytus bucket {create,ls,push,pull,grant,revoke}`
   subcommand.
2. A tray-menu cell summarising bucket count + capacity + the
   garagetytus version.

## Subcommand contract

`tytus bucket *` MUST forward its args to `garagetytus bucket *`
verbatim, with one twist: it resolves the right endpoint per
caller context (Mac-local vs pod) the same way
`garagetytus.CallerContext.from_runtime()` does in Python.

For Mac-local calls — direct exec of `garagetytus bucket *`. The
inherited stdio pattern from Makakoo's wrapper applies (see
`makakoo-os/makakoo/src/commands/bucket.rs` Phase D commit `ae97464`).

For pod-side calls — the existing tytus tunnel + Ed25519 envelope
infrastructure carries the request to the Mac-side
`<wg-ip>:8765` shim. Makakoo's MCP shim must be running for
pod-side reach (documented limitation; v0.2 may widen).

### Flag matrix

| tytus subcommand | garagetytus shell-out |
|---|---|
| `tytus bucket create my-data --ttl 7d --quota 10G` | `garagetytus bucket create my-data --ttl 7d --quota 10G` |
| `tytus bucket ls` | `garagetytus bucket list --json` (pretty-printed by tytus) |
| `tytus bucket push <local> s3://my-data/<key>` | `garagetytus bucket grant my-data --to "tytus-cli-tx" --perms read,write --ttl 5m --json` → `aws s3 cp` with the returned creds |
| `tytus bucket pull s3://my-data/<key> <local>` | similar to push but reverse |
| `tytus bucket grant my-data --to <label> [--ttl <d>] [--perms <p>]` | `garagetytus bucket grant ...` (passthrough) |
| `tytus bucket revoke <grant-id>` | `garagetytus bucket revoke <grant-id>` |

`tytus bucket push/pull` adds value over the bare `garagetytus
bucket grant + aws s3 cp` two-step by chaining them — the user
gets a single command that mints a short-lived grant, executes
the transfer, revokes the grant on completion. Mac-local only;
pod-side is queued for tytus v0.5.

### Error contract

If `garagetytus` is not installed, `tytus bucket *` MUST surface
the same Phase D.3 fallback message:

```
garagetytus not found — install at https://garagetytus.dev
```

…on stderr, exit non-zero. **No silent fallback to a tytus-
embedded Garage path** (LD#1).

## Tray-menu cell contract

```
🪣 Buckets — N total, M.MM GB / Q GB                (status: OK / RO)
   ├─ Open in Filebrowser                              [opens browser]
   ├─ Show recent grants…                              [submenu]
   └─ Garagetytus version: 0.1                         [click → repo]
```

Numbers come from `garagetytus capabilities --json`; OK/RO badge
comes from `garagetytus status --json` (LD#11 watchdog protocol —
reads `<state-dir>/watchdog.json` and surfaces the
`mode = "rw"` / `"ro"` flag).

Cells refresh every 30 s; the `tytus-tray` daemon polls the
JSON surfaces, no IPC socket required (LD#11).

## v0.1 scope and v0.2 outlook

- v0.1 ships **Mac-only** for tytus-side reasons (tytus's binary
  is Mac-only Mach-O today). Once tytus-cli ships Linux/Windows,
  the adapter widens automatically — `tytus bucket` is platform-
  agnostic Rust orchestration around a platform-agnostic
  `garagetytus` child.
- v0.2 considerations: pod-side `bucket push/pull` chaining,
  multi-pod buckets (one bucket reachable from N pods at once).

## Getting started

For the tytus team picking this up:

1. Add `commands::bucket` module to the tytus CLI repo.
2. Surface the 7 subcommands from §"Flag matrix" above.
3. Wire stdio inheritance per pi/codex G1 (Makakoo Q2 verdict).
4. Add `bucket` cell to `tytus-tray` menu module.
5. Publish a v0.5 tag.

PRs against `garagetytus` for spec clarifications are welcome.
The garagetytus side does NOT need any tytus-aware code (LD#12
forbids that direction); changes here are spec-only.
