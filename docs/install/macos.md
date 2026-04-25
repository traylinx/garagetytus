# Install — macOS

## Recommended: Homebrew

```bash
brew install traylinx/tap/garagetytus
```

The formula declares `depends_on "garage"`, so Homebrew also
installs the upstream `garage` AGPL formula (which compiles from
the source tarball at
`git.deuxfleurs.fr/Deuxfleurs/garage` v2.3.0, sha256 pinned by
the formula).

## Alternative: web bootstrap

```bash
curl -fsSL garagetytus.dev/install | sh
```

Same result, no brew dependency. Drops the binary at
`~/.local/bin/garagetytus`. You may need to add that to PATH:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

The web bootstrap requires `garage` to be on PATH — install via
brew first, OR via [Garage's own
install](https://garagehq.deuxfleurs.fr/documentation/quick-start/),
OR via Linux musl binary if you're really determined.

## After install

```bash
garagetytus install      # detects brew + garage, generates plist + config
garagetytus start        # launchctl load
garagetytus bootstrap    # admin-API setup + creds in macOS Keychain
```

## Path layout

| Item | Path |
|---|---|
| Binary | `/usr/local/bin/garagetytus` (brew Intel) or `/opt/homebrew/bin/garagetytus` (brew Apple Silicon) or `~/.local/bin/garagetytus` (web bootstrap) |
| Garage binary | `/usr/local/bin/garage` / `/opt/homebrew/bin/garage` (brew) |
| Config | `~/Library/Application Support/garagetytus/garagetytus.toml` |
| Data dir | `~/Library/Application Support/garagetytus/data/` |
| Logs | `~/Library/Logs/garagetytus/` |
| launchd plist | `~/Library/LaunchAgents/com.traylinx.garagetytus.plist` |
| Grants | `~/.garagetytus/grants.json` |
| Keychain entry | service `garagetytus`, account `s3-service` |

## Uninstall

```bash
garagetytus uninstall              # removes daemon, service, creds
garagetytus uninstall --keep-data  # leave bucket data on disk
brew uninstall traylinx/tap/garagetytus
brew uninstall garage              # optional — remove the upstream Garage too
```

## Troubleshooting

**Port 3900 already in use** — bootstrap surfaces the offending
PID with a `--api-port` override hint. No silent bind-failure
(AC4 acceptance contract).

**SmartScreen / Gatekeeper** — release binaries are signed +
notarized by traylinx. If the notarization is missing, fall back
to brew (formula installs from source, no notarization required).
