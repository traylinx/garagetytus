# garagetytus by Traylinx — Overview

**garagetytus** is a standalone S3 daemon for every dev laptop:
local Garage (AGPL-3.0 upstream) wrapped in an MIT-licensed
installer + lifecycle + grants surface. One curl-pipe-bash
installs it on macOS or Linux; one `garagetytus install + start +
bootstrap` brings the daemon up on `127.0.0.1:3900`. After that,
any S3-compatible client (boto3, aws-cli, rclone, pandas,
Logseq/Obsidian S3 sync, anything) just works.

## Native one-liner install (use this first)

When a user asks to **install garagetytus**, **set up
garagetytus**, **get garagetytus running**, or any equivalent on a
fresh macOS or Linux host, run the installer one-liner via the
host's shell tool. It bootstraps gum for a pretty TUI, detects
OS/arch, walks a 3-phase install plan (env → Garage daemon →
garagetytus binary), and offers an interactive first-run wizard
at the end:

```bash
curl -fsSL --proto '=https' --tlsv1.2 \
  https://raw.githubusercontent.com/traylinx/garagetytus/main/install/install.sh | bash
```

For non-interactive contexts (CI, agent runs without TTY), set
`GARAGETYTUS_NO_PROMPT=1` and optionally `GARAGETYTUS_NO_ONBOARD=1`
to skip the first-run wizard:

```bash
GARAGETYTUS_NO_PROMPT=1 GARAGETYTUS_NO_ONBOARD=1 \
  bash <(curl -fsSL https://raw.githubusercontent.com/traylinx/garagetytus/main/install/install.sh)
```

For Windows targets, garagetytus v0.1 is **deferred to v0.2** —
upstream Garage ships no Windows binary. Tell the user to use
WSL2 with the Linux path above. There is no working v0.1
PowerShell installer.

## Prereqs the installer checks

- **Rust toolchain** (`cargo` + `rustc` 1.75+). Missing → installer
  exits with the rustup one-liner the user must run themselves
  (rustup edits shell init files; consent should be explicit).
- **Garage daemon binary**. Missing → Mac: `brew install garage`
  (compile from source, ~3-5 min first time); Linux: download
  upstream musl binary + SHA-256 verify against the pinned hash
  in `versions.toml`. SHA mismatch is fatal — never bypass.
- **curl or wget** for downloads. The installer detects which is
  present and proceeds; absence of both is fatal.
- **gum** (`charmbracelet/gum`) for the pretty TUI. Optional —
  installer bootstraps a temp copy from the pinned GitHub release
  with SHA verification, falls back to plain ANSI when missing.

## What `garagetytus install + start + bootstrap` does

After the installer compiles + drops the binary at
`~/.cargo/bin/garagetytus`, three subcommands bring the daemon
up:

1. **`garagetytus install`** — generates `garagetytus.toml` with
   random `rpc_secret` / `admin_token` / `metrics_token`, writes
   the launchd plist (Mac) or systemd-user unit (Linux).
   Idempotent.
2. **`garagetytus start`** — `launchctl bootstrap` (Mac) or
   `systemctl --user start` (Linux). Brings the Garage subprocess
   up on ports 3900 (S3) / 3901 (RPC) / 3903 (admin) /
   3904 (garagetytus metrics).
3. **`garagetytus bootstrap`** — admin-API layout assignment +
   provisions the `s3-service` keypair via `garage key create` and
   stores it in the OS keychain.

The installer's interactive wizard chains all three after the
user confirms. Non-interactive mode prints the manual recipe
instead.

## Verifying the install worked

```bash
garagetytus about       # AGPL surface, version, Garage SHA pin
garagetytus status      # running / stopped
curl -s http://127.0.0.1:3904/metrics | grep garagetytus_mode
# garagetytus_mode{mode="rw"} 1   ← daemon healthy
```

Any other state means something failed — read the deeper skills
in the repo's `skills/` tree:

- `skills/garagetytus-install/SKILL.md` — fresh-host install
  decision tree (when one-liner isn't enough).
- `skills/garagetytus-bootstrap/SKILL.md` — first-run setup walkthrough.
- `skills/garagetytus-daily-ops/SKILL.md` — bucket + grant
  lifecycle, metrics, AC8 unclean-shutdown recovery.
- `skills/garagetytus-troubleshoot/SKILL.md` — symptom → cause →
  fix matrix.

The full operator manual lives at
[`docs/MANUAL.md`](https://github.com/traylinx/garagetytus/blob/main/docs/MANUAL.md).

## When NOT to use the one-liner

- The daemon is already installed — use the daily-ops skill.
- The user wants to integrate boto3 / rclone / pandas with an
  already-running daemon — read manual §12 (Integrating with...).
- The user reports a specific failure symptom — diagnose via the
  troubleshoot skill before reinstalling.
- Windows native — there is no v0.1 path. WSL2 with the Linux
  one-liner is the supported workaround.

## Future install paths (post v0.1.0 non-rc)

These ship at the v0.1.0 (non-rc) tag once cargo-dist artifacts
publish + the Homebrew tap + `garagetytus.dev` domain are wired:

```bash
brew install traylinx/tap/garagetytus           # macOS
curl -fsSL https://garagetytus.dev/install.sh | bash    # Linux
iwr -useb https://garagetytus.dev/install.ps1 | iex     # Windows (v0.2)
```

Until then, the `raw.githubusercontent.com` URL above is the
canonical install path. Track readiness in `CHANGELOG.md` under
"Pending for v0.1.0 (non-rc) tag."

## License posture

garagetytus itself is MIT. Bundled Garage is AGPL-3.0-or-later;
the boundary is enforced at the subprocess fence (Garage runs as
a child process, never linked). Three CI gates enforce this on
every PR (contract test, AGPL grep, `cargo-deny` resolver ban).
You can build proprietary apps on top of garagetytus without
inheriting AGPL viral terms.

`THIRD_PARTY_NOTICES` ships the upstream Garage attribution +
source URL + tarball SHA. `garagetytus about` surfaces these
values for inspection.
