# Install — Windows

> **v0.1 ships macOS + Linux only.** Windows targets v0.2.

## Why deferred

Per Q1 verdict (lope, pi+codex 2026-04-25 — both PASS Option A,
see [`verdicts/Q1-VERDICT.md`](../../verdicts/Q1-VERDICT.md)):

Garage upstream ships **Linux binaries only** across all v2.x
releases (4 musl targets: `x86_64`, `aarch64`, `armv6l`, `i686`).
**No Windows binary exists anywhere upstream** — release builds,
extra builds, dev builds, all Linux.

Building our own Windows binary from upstream Garage source
requires:
- Cross-compilation toolchain (Rust MinGW or MSVC targets).
- Authenticode code-signing infrastructure.
- Tag-driven CI matching every Garage upstream release.
- Ongoing maintenance.

The v0.1 budget can't carry that pipeline. Windows is renegotiated
to v0.2 with three explicit options on the table:
1. Build our own Garage Windows binary via CI.
2. WSL2 path (treat Windows as a Linux container).
3. Drop the Windows lane permanently and document
   "Linux + macOS only."

## Today's options

If you're on Windows and need a local S3 daemon **right now**:

### WSL2 + the Linux path (recommended)

```powershell
wsl curl -fsSL garagetytus.dev/install | sh
wsl garagetytus install
wsl garagetytus start
wsl garagetytus bootstrap
```

The daemon runs inside WSL2 and is reachable from your Windows
host on `127.0.0.1:3900` (WSL2 networking forwards localhost by
default on recent versions).

**Caveat**: this is unsupported in v0.1. v0.2 will pick a first-
class path; that may or may not be WSL2.

### Docker Desktop

Pull a generic Linux container with `garagetytus` pre-installed.
Same caveat as WSL2 — unsupported in v0.1.

### Alternative S3-compatible product

If you need supported Windows S3 storage today, look at:
- [MinIO](https://min.io) (Windows binary available, AGPLv3).
- [S3rver](https://github.com/jamhall/s3rver) (Node, dev-only).

## What v0.1 install.ps1 does

```powershell
irm garagetytus.dev/install.ps1 | iex
```

Prints the deferral notice + the WSL2 / Docker workaround
pointers and exits 0. **Planned deferral, not an error** — the
exit 0 means scripts that pipe-iex don't break.

Track v0.2 progress: <https://github.com/traylinx/garagetytus/issues?q=label%3Av0.2>
