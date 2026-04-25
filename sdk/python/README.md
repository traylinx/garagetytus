# garagetytus-sdk

Python SDK for the [garagetytus](https://garagetytus.dev) local S3
daemon.

## Install

```bash
pip install garagetytus-sdk
```

The SDK assumes:
- `garagetytus` daemon is running on `127.0.0.1:3900` (or via the
  pod-side shim — auto-detected via `GARAGETYTUS_PEER_NAME` /
  `MAKAKOO_PEER_NAME` env var).
- A bootstrapped `s3-service` keypair lives in the OS keychain
  under service name `garagetytus`.

Run `garagetytus install && garagetytus start && garagetytus
bootstrap` first if you haven't.

## Usage

```python
from garagetytus import client, bucket_ctx

# Default — Mac-local, picks up creds from keychain.
s3 = client()
s3.put_object(Bucket="foo", Key="bar.txt", Body=b"hello")

# Per-grant credentials (returned by `garagetytus bucket grant`).
s3 = client(access_key="GK...", secret_key="...")

# Context-manager with auto bucket-create.
with bucket_ctx("foo") as b:
    b.put("bar.txt", b"hello")
    for key in b.list():
        print(key)
```

## Caller context

Mac-local callers reach the daemon at `127.0.0.1:3900`. Pod-originated
callers go via the MCP shim at `<pod-wg-ip>:8765`. The SDK
auto-detects via `GARAGETYTUS_PEER_NAME` / `MAKAKOO_PEER_NAME`.

## License

MIT — see `LICENSE` at the repo root.
