# Quickstart

5 minutes from "never heard of garagetytus" to "S3-on-localhost
with a sub-keypair scoped to your app."

## 1. Install

**macOS:**
```bash
brew install traylinx/tap/garagetytus
```

**Linux:**
```bash
curl -fsSL garagetytus.dev/install | sh
```

**Windows:**
v0.2 — see [Q1 verdict](../../verdicts/Q1-VERDICT.md).

## 2. Bring the daemon up

```bash
garagetytus install      # wires Garage + service registration
garagetytus start        # daemon comes up on 127.0.0.1:3900 + :3903
garagetytus bootstrap    # admin-API layout assignment + creds in keychain
```

`garagetytus install` is idempotent — running it twice is a
no-op the second time.

## 3. Create a bucket

```bash
garagetytus bucket create my-data --ttl 7d --quota 1G
```

TTL grammar: `30m | 1h | 24h | 7d | permanent`. Quota: `100M`,
`1G`, `10G`, or `unlimited` (`unlimited` requires
`--confirm-yes-really`).

## 4. Mint a per-app grant

```bash
garagetytus bucket grant my-data \
    --to "my-python-app" \
    --perms read,write \
    --ttl 1h \
    --json
```

You get back a JSON envelope:

```json
{
  "grant_id": "g_20260425_a3f9c12d",
  "access_key": "GK1234567890abcdef",
  "secret_key": "...",
  "endpoint_url": "http://127.0.0.1:3900",
  "expires_at": "2026-04-25T13:42:00Z"
}
```

Wire those into your app — see
[docs/integrate/external-app.md](../integrate/external-app.md).

## 5. (When done)

```bash
garagetytus bucket revoke g_20260425_a3f9c12d
```

Or wait for the TTL — expired grants are purged automatically by
the watchdog.

## Status / health

```bash
garagetytus status                # service Running / Stopped
curl http://127.0.0.1:3903/metrics  # Prometheus text (LD#11)
cat ~/.garagetytus/watchdog.json    # JSON mirror of the metrics
```
