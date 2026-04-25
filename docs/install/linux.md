# Install — Linux

## Recommended: web bootstrap

```bash
curl -fsSL garagetytus.dev/install | sh
```

Detects target triple (`x86_64-unknown-linux-musl` or
`aarch64-unknown-linux-musl`), downloads the right binary from
`github.com/traylinx/garagetytus/releases/latest`, drops it at
`~/.local/bin/garagetytus`. Pin the bootstrap script SHA from
the release page if you're a "no curl-pipe-sh" shop.

## After install

```bash
garagetytus install        # downloads upstream Garage musl binary,
                           # SHA-verifies, installs systemd-user unit
garagetytus start          # systemctl --user start garagetytus
garagetytus bootstrap      # admin-API setup + creds in Secret Service
```

The Linux installer pulls upstream Garage v2.3.0
(`garagehq.deuxfleurs.fr/_releases/v2.3.0/<target>/garage`),
SHA-256 verified against the pinned table in `versions.toml`.
Per LD#1, garagetytus never links against any `garage-*` Rust
crate — it's a child process via `Command::new("garage")`.

The musl binary is statically linked, so it runs on glibc hosts
too. Distros tested: Ubuntu 22.04, Debian 12, Alpine 3.18, Arch
2026.04.

## Path layout (XDG)

| Item | Path |
|---|---|
| Binary | `~/.local/bin/garagetytus` |
| Garage binary | `~/.local/bin/garage` (downloaded by `garagetytus install`) |
| Config | `$XDG_CONFIG_HOME/garagetytus/garagetytus.toml` (default `~/.config/garagetytus/`) |
| Data dir | `$XDG_DATA_HOME/garagetytus/data/` (default `~/.local/share/garagetytus/data/`) |
| Logs | `$XDG_DATA_HOME/garagetytus/logs/` |
| systemd unit | `~/.config/systemd/user/garagetytus.service` |
| Grants | `~/.garagetytus/grants.json` |
| Keychain | Secret Service via `keyring` crate, service `garagetytus`, account `s3-service` |

## Headless Linux (no Secret Service)

`garagetytus install` refuses by default if Secret Service isn't
running (no silent fallback — LD#6). Use:

```bash
garagetytus install --allow-file-creds
```

…to fall back to a plaintext `creds.json` in the config dir.
Less secure; documented per LD#6.

## Uninstall

```bash
garagetytus uninstall              # removes daemon + creds + systemd unit
garagetytus uninstall --keep-data  # leave bucket data on disk
rm ~/.local/bin/garagetytus ~/.local/bin/garage
```

## Troubleshooting

- **`garagetytus install` says "Secret Service not running"**:
  install a Secret Service daemon (gnome-keyring, kwallet, pass)
  OR use `--allow-file-creds`.
- **Port 3900 collision**: pass `--api-port` to override.
- **systemd --user not enabled**: `loginctl enable-linger $USER`
  to keep user-services running across logouts.
