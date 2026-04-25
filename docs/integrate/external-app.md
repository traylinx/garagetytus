# Integrating with garagetytus from your own app

> "I have a Python/Node/Rust/Go app that wants to talk to a local
> S3 daemon. How does garagetytus fit?"

The short answer: **garagetytus is just S3 on `127.0.0.1:3900`.**
Anything that speaks the S3 wire protocol — boto3, aws-cli,
rclone, MinIO clients, custom Go S3 SDKs — points at that
endpoint, uses path-style addressing, and works.

## Quickstart (Python)

```bash
brew install traylinx/tap/garagetytus           # macOS
# OR
curl -fsSL garagetytus.dev/install | sh         # Linux

garagetytus install
garagetytus start
garagetytus bootstrap

garagetytus bucket create my-data --ttl 7d --quota 1G
GRANT=$(garagetytus bucket grant my-data --to "my-app" \
        --perms read,write --ttl 1h --json)
ACCESS=$(echo "$GRANT" | jq -r .access_key)
SECRET=$(echo "$GRANT" | jq -r .secret_key)
```

Then in your app:

```python
import boto3
from botocore.config import Config

s3 = boto3.client(
    "s3",
    endpoint_url="http://127.0.0.1:3900",
    region_name="garage",
    aws_access_key_id="<ACCESS>",
    aws_secret_access_key="<SECRET>",
    config=Config(s3={"addressing_style": "path"}),
)
s3.put_object(Bucket="my-data", Key="hello.txt", Body=b"world")
```

## Even shorter — use the `garagetytus-sdk` pip package

```bash
pip install garagetytus-sdk
```

```python
from garagetytus import bucket_ctx

with bucket_ctx("my-data", grant_id="g_20260425_abc12def") as b:
    b.put("hello.txt", b"world")
    for key in b.list():
        print(key)
```

The SDK auto-resolves credentials from the OS keychain (macOS
Keychain, Linux Secret Service, Windows Credential Manager) and
the endpoint URL from caller context (Mac-local vs pod).

## Path-style addressing (LD#4)

garagetytus locks **path-style** addressing
(`http://endpoint/<bucket>/<key>`), never virtual-host style
(`http://<bucket>.endpoint/<key>`). This is non-negotiable —
it's the only addressing style guaranteed to work with
`127.0.0.1` endpoints.

Every SDK call that constructs a boto3 client must include:

```python
config=Config(s3={"addressing_style": "path"})
```

or its native-language equivalent. Virtual-host requests get
HTTP 400 from the shim.

## Rust, Go, JS, etc.

Any S3-compatible SDK works. Make sure you set:
- Endpoint URL: `http://127.0.0.1:3900`
- Region: `garage`
- Path-style addressing.

For Rust, [`aws-sdk-s3`](https://docs.rs/aws-sdk-s3) with a
custom endpoint resolver. For Go,
[`github.com/aws/aws-sdk-go-v2`](https://github.com/aws/aws-sdk-go-v2)
ditto. For Node, the AWS JS SDK with `forcePathStyle: true`.

## Grant credentials lifecycle

```bash
# Mint a 1h grant.
garagetytus bucket grant my-data --to "my-app" --perms read,write --ttl 1h --json

# Revoke when done.
garagetytus bucket revoke g_20260425_abc12def
```

Grants live in `~/.garagetytus/grants.json` (Mac/Linux) or
`%APPDATA%\garagetytus\grants.json` (Windows v0.2). The file is
machine-local, gitignored, never synced. Sebastian's Makakoo
host + tytus pod all read this same file (read-only) and
honour the grants — see `docs/integrate/{makakoo,tytus}.md`.

## Watchdog signals

For app-side health monitoring (see your own status page,
metrics dashboard, etc.), garagetytus exposes:

- `GET http://127.0.0.1:3903/metrics` — Prometheus text format
  (LD#11). Counters/gauges include disk-free percentage, `mode`
  (rw|ro), uptime, unclean-shutdown total, watchdog-tick unix
  seconds.
- `<state-dir>/watchdog.json` — atomic-write JSON mirror of the
  metrics for callers that don't speak Prometheus.

Poll either surface; no IPC socket in v0.1.

## License + AGPL

garagetytus itself is MIT (see `LICENSE`). The bundled Garage
S3 daemon is AGPL-3.0-or-later. garagetytus orchestrates Garage
as a child process and never links against any garage-* crate
(LD#1, hard-fail CI gate). For your app: as long as you talk to
Garage over the S3 wire protocol, you have zero AGPL exposure.

`THIRD_PARTY_NOTICES` at the repo root has the full upstream
attribution.
