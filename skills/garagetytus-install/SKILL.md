---
name: garagetytus-install
version: 0.1.0
description: |
  Install the garagetytus standalone S3 daemon on a fresh Mac or
  Linux host end-to-end. Detects the platform, walks the prereq
  chain (Garage upstream binary, Homebrew on Mac, musl download +
  SHA verify on Linux), runs `garagetytus install`, and
  post-verifies the seeded state. Refuses to proceed on prereq
  gaps with concrete fix instructions.
allowed-tools:
  - Bash
  - Read
category: infrastructure
tags:
  - garagetytus
  - install
  - bootstrap
  - macos
  - linux
  - s3
---

# garagetytus-install — fresh-host install workflow

Use this skill when a user says **"install garagetytus"**, **"set
up garagetytus"**, **"get garagetytus running"**, or any equivalent
on a host that doesn't currently have the daemon installed.

## When to reach for it

- The user wants S3 on `127.0.0.1:3900` and has nothing installed.
- A new dev machine is being provisioned.
- An existing install is broken at the binary level (corrupt brew
  formula, missing `garage`, etc.) and needs a clean re-install.

## When NOT to reach for it

- garagetytus is already installed and running — use
  `garagetytus-daily-ops` instead.
- The user wants to bootstrap an already-installed daemon — use
  `garagetytus-bootstrap`.
- The user is on Windows — there's no v0.1 install path. Tell them
  Windows is deferred to v0.2 and offer the WSL2 Linux path.
- Symptom is "daemon won't start" or "metrics 503" — diagnose via
  `garagetytus-troubleshoot` first; reinstall only as last resort.

## Decision tree

### Step 1 — Platform detect

```bash
uname -s
```

Branch on output:
- `Darwin` → step 2 (Mac).
- `Linux` → step 3 (Linux).
- `MINGW*` / `CYGWIN*` / `MSYS*` → tell the user Windows is v0.2,
  offer the WSL2 Linux path, **stop**.

### Step 2 — macOS install

**Prereq probe:**

```bash
which brew                     # must be on PATH
which garage 2>/dev/null       # may or may not be present
```

If `brew` missing → tell the user to install Homebrew first
(`https://brew.sh`). Don't try to install brew yourself; that
needs sudo + interactive consent. **Stop.**

If `brew` present, install garagetytus + (transitively) garage:

```bash
brew install traylinx/tap/garagetytus
```

This pulls in `garage` as a dependency (compiled from source,
~3–5 min first time). Wait for it to finish. Then run the
in-binary installer:

```bash
garagetytus install
```

This step is idempotent — safe to retry on transient failure.

### Step 3 — Linux install

**Prereq probe:**

```bash
uname -m                       # x86_64 or aarch64
curl --version | head -1       # must be present
which systemctl                # user-mode systemd required
```

If `systemctl` missing → tell the user `garagetytus` requires
systemd-user (the alternative is `garagetytus serve` under their
own supervisor; ask which they want).

Run the web bootstrap, then the in-binary installer:

```bash
curl -fsSL https://garagetytus.dev/install | sh
garagetytus install
```

The web bootstrap downloads the `garagetytus` binary itself; the
`install` step downloads the upstream `garage` musl binary, verifies
its SHA against the pin in `versions.toml`, and drops it at
`~/.local/bin/garage`. SHA mismatch is fatal — refuse to proceed and
tell the user to file an issue (do NOT attempt to bypass the
check).

### Step 4 — Post-install verification

Always run all three after either platform's install:

```bash
garagetytus about              # AGPL surface + version pin
garagetytus status             # should print "stopped"
ls "$(dirname "$(garagetytus --config-path 2>/dev/null || echo ~/.config/garagetytus/garagetytus.toml)")"
```

Expected:
- `garagetytus about` prints version + Garage upstream URL +
  tarball SHA. **Non-empty output = success.**
- `garagetytus status` prints `stopped` (it's installed but not
  running yet — that's correct at this stage).
- The config directory contains `garagetytus.toml` and either a
  plist (Mac) or systemd unit reference (Linux).

### Step 5 — Hand off

If everything green, tell the user the daemon is **installed but
not bootstrapped**. Two next moves:

```bash
garagetytus start         # daemon comes up
garagetytus bootstrap     # admin-API layout + service keypair
```

Or invoke the `garagetytus-bootstrap` skill to walk the user
through that step.

## Failure mode handling

| Symptom | Cause | Fix |
|---|---|---|
| `brew: command not found` | Homebrew missing on Mac | Tell user to install brew at `https://brew.sh`. Stop — needs sudo. |
| `Error: traylinx/tap not found` | Tap not added | `brew tap traylinx/tap` then retry. |
| Linux SHA mismatch | Pinned hash drifted from upstream tarball | **Refuse to proceed.** File an issue. Do not bypass. |
| `port 3900 already in use` during eventual `start` | Existing daemon (Makakoo's bundled Garage, manual install, etc.) | Hand off to `garagetytus-troubleshoot` symptom "port collision". |
| `garage: command not found` after `brew install` | brew formula didn't pull garage | `brew install garage` directly, then `garagetytus install`. |
| `garagetytus install` exits non-zero | Various — read stderr | If config tokens already exist, that's idempotent success. Otherwise relay the error to the user verbatim. |

## Idempotence guarantee

Every step in this skill is safe to re-run. `brew install` of an
already-present formula is a no-op. `garagetytus install` preserves
existing tokens and skips already-present files. A failed install
+ retry cycle leaves no half-state to clean up.

## What this skill does NOT do

- **Bootstrap the daemon.** That's a separate workflow because it
  requires the daemon to be running first. See
  `garagetytus-bootstrap`.
- **Create buckets or grants.** Once bootstrapped, see
  `garagetytus-daily-ops`.
- **Install on Windows.** v0.1 ships Mac + Linux only. Hard refuse;
  offer WSL2 as the v0.1 workaround.
- **Bypass the SHA verification.** Linux SHA mismatch is fatal by
  design — supply-chain integrity gate.

## See also

- Manual: `docs/MANUAL.md` §3 (Mac) + §4 (Linux) + §5 (Windows
  deferral).
- `garagetytus-bootstrap` — the natural next step after install.
- `docs/install/{macos,linux,windows}.md` — per-OS prose notes.
