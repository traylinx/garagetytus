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

## Documentation

- **[`docs/MANUAL.md`](docs/MANUAL.md)** — end-to-end operator
  manual. Architecture, install on Mac/Linux, bootstrap, bucket
  + grant lifecycle, observability, recovery from unclean
  shutdown, configuration reference, uninstall, AGPL posture.
  Read this end-to-end the first time.
- [`docs/usage/quickstart.md`](docs/usage/quickstart.md) —
  five-minute primer.
- [`docs/usage/grants.md`](docs/usage/grants.md) — grant grammar
  reference.
- [`docs/install/{macos,linux,windows}.md`](docs/install/) —
  per-OS install notes.
- [`docs/integrate/{makakoo,tytus,external-app}.md`](docs/integrate/)
  — integration contracts.

## Agent skills

Each `skills/<name>/SKILL.md` is an agent-readable decision tree
covering one workflow. Any AI CLI (Claude / Gemini / Codex / pi /
…) can be told **"install garagetytus"** or **"the daemon won't
start"** and will follow the matching skill autonomously.

- [`skills/garagetytus-install/`](skills/garagetytus-install/SKILL.md)
  — fresh-host install on Mac or Linux. Prereq detection, error
  recovery, post-install verification.
- [`skills/garagetytus-bootstrap/`](skills/garagetytus-bootstrap/SKILL.md)
  — first-run bootstrap (admin-API layout + service keypair).
- [`skills/garagetytus-daily-ops/`](skills/garagetytus-daily-ops/SKILL.md)
  — start/stop/restart, bucket + grant lifecycle, metrics +
  watchdog, unclean-shutdown recovery.
- [`skills/garagetytus-troubleshoot/`](skills/garagetytus-troubleshoot/SKILL.md)
  — symptom → cause → fix matrix for every failure mode in
  install, bootstrap, lifecycle, and S3-client integration.

For Makakoo users, `makakoo plugin install
git+https://github.com/traylinx/garagetytus.git` makes these
skills discoverable via `skill_discover` automatically.

## Repo layout

```
garagetytus/
├── crates/
│   ├── garagetytus/            # CLI binary
│   ├── garagetytus-core/       # paths · keychain · backend trait (LD#13)
│   ├── garagetytus-grants/     # user-grants · rate-limit · audit
│   └── garagetytus-watchdogs/  # disk · integrity · keychain-migrate
├── sdk/python/                 # garagetytus-sdk pip package
├── docs/                       # manual + per-OS + integration
├── skills/                     # agent-facing SKILL.md decision trees
├── install/                    # install.sh / install.ps1 / homebrew formula
├── deny.toml                   # cargo-deny gate (LD#1 + license allowlist)
├── versions.toml               # per-target Garage upstream SHA pins
└── THIRD_PARTY_NOTICES         # Garage AGPL attribution
```

## License

MIT. Bundled Garage is AGPL-3.0-or-later — see `THIRD_PARTY_NOTICES`
once Phase B.5 lands.

## Powered by

[![Powered by garagetytus](https://img.shields.io/badge/powered_by-garagetytus-3a3a3a)](https://github.com/traylinx/garagetytus)
