# garagetytus

> Local S3 daemon for every dev laptop. Standalone Mac + Linux. Powered
> by [Garage](https://garagehq.deuxfleurs.fr/) under the hood, but
> garagetytus owns the install + daemon lifecycle + grants surface.

**Status: v0.1 in development (2026-04-25).**

## Install

```bash
brew install traylinx/tap/garagetytus           # macOS
curl -fsSL garagetytus.dev/install | sh         # Linux

garagetytus install && garagetytus start && garagetytus bootstrap
garagetytus bucket create my-data --ttl 7d --quota 1G
garagetytus bucket grant my-data --to "external-app" --perms read,write --ttl 1h
```

Then any S3-compatible client points at `http://127.0.0.1:3900`:
boto3, aws-cli, rclone, pandas, Logseq S3-sync, anything.

**Windows targets v0.2** (lope verdict 2026-04-25 — Garage upstream
ships no Windows binary; v0.1 budget can't carry a Windows build
pipeline. Reopens at v0.2.)

## What this is

A single-binary daemon that wraps Garage (AGPL upstream S3 daemon)
with:

- A cross-platform installer (`garagetytus install`).
- Lifecycle commands (`garagetytus {start,stop,status,restart}`).
- Bucket primitives (`garagetytus bucket {create,ls,grant,revoke}`).
- A user-grants store at `~/.garagetytus/grants.json` that Makakoo +
  tytus consume (read-only) without re-implementing.
- Watchdogs (disk-space, integrity, keychain migrate) baked into the
  daemon process — no external supervisor required.

Garage stays a child process. Never linked. AGPL boundary clean.

## Repo layout

```
garagetytus/
├── crates/
│   └── garagetytus-grants/    # user-grants store (carved 2026-04-25)
└── (more lands during Phase A.1–F)
```

## License

MIT. Bundled Garage is AGPL-3.0-or-later — see `THIRD_PARTY_NOTICES`
once Phase B.5 lands.

## Powered by

[![Powered by garagetytus](https://img.shields.io/badge/powered_by-garagetytus-3a3a3a)](https://github.com/traylinx/garagetytus)
