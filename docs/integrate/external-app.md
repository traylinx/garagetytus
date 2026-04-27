# Integrating with garagetytus from your own app

> "I have a Python/Node/Rust/Go app that wants to talk to a local
> S3 daemon. How does garagetytus fit?"

There are two flavors:

**Flavor A — local laptop daemon.** Standalone garagetytus binds
`127.0.0.1:3900` on your dev machine. No internet, no auth surface,
single user. Use this for offline dev / CI / "S3 in a box."

**Flavor B — Tytus shared service.** One garagetytus daemon on the
Tytus droplet, fronted by Caddy + Let's Encrypt at
`https://garagetytus.traylinx.com`. Multi-tenant: per-bucket
SigV4 keys minted by the Tytus orchestrator. Use this when you need
to share files between Sebastian's Mac, multiple Tytus pods, and
external clients (customer Macs, CI, third-party agents) without
WireGuard.

Both flavors speak the same S3 wire protocol. boto3, aws-cli,
rclone, MinIO clients, custom Go SDKs — point them at the relevant
endpoint, use path-style addressing, sign with SigV4 (`region: garage`),
and they work. Only `endpoint_url` and the credentials change.

---

## Flavor A — local laptop daemon

### Quickstart (Python)

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

---

## Flavor B — Tytus shared service via `garagetytus.traylinx.com`

> Live since 2026-04-26.

The Tytus team operates one garagetytus daemon on a Strato droplet
and exposes it publicly through Caddy + Let's Encrypt. Per-bucket
SigV4 keys are minted by the Tytus orchestrator at pod-allocation
time. Same daemon, same wire protocol, two reach paths:

| Reach path | Endpoint | When to use |
|---|---|---|
| Inside a Tytus pod | `http://10.42.42.1:3900` | Pod-side agents (NemoClaw, etc.) — already wired by `garagetytus-pod-provision`. |
| Outside (anywhere with internet) | `https://garagetytus.traylinx.com` | Customer Macs, third-party SDKs, CI runners, agents not in a pod. |

### Quickstart (Python — external client)

You receive an `access_key` + `secret_access_key` pair from
whoever provisioned your bucket. They look like
`GK6e7b459e9fe995a67e1fca6c` and
`160b5fe40d943794f76e48b082535d972f8ddfaf6bca752a37d09282bbf73610`
— Garage-issued, scoped to one bucket, often time-limited.

```python
import boto3
from botocore.config import Config

s3 = boto3.client(
    "s3",
    endpoint_url="https://garagetytus.traylinx.com",
    region_name="garage",
    aws_access_key_id="<ACCESS>",
    aws_secret_access_key="<SECRET>",
    config=Config(
        s3={"addressing_style": "path"},
        signature_version="s3v4",
    ),
)

# List
for obj in s3.list_objects_v2(Bucket="<bucket>").get("Contents", []):
    print(obj["Key"], obj["Size"])

# Put
s3.put_object(Bucket="<bucket>", Key="from-mac/hello.txt", Body=b"hi")

# Presigned download URL (valid 2 minutes, no auth needed by recipient)
url = s3.generate_presigned_url(
    "get_object",
    Params={"Bucket": "<bucket>", "Key": "from-mac/hello.txt"},
    ExpiresIn=120,
)
```

### Quickstart (rclone — external client)

```bash
rclone config create garagetytus-public s3 \
    provider=Other \
    endpoint=https://garagetytus.traylinx.com \
    region=garage \
    access_key_id=<ACCESS> \
    secret_access_key=<SECRET>

rclone ls garagetytus-public:<bucket>
rclone copy ./local-file.txt garagetytus-public:<bucket>/from-mac/
```

### Quickstart (curl — external client)

For the rare case where you want to drive S3 from `curl`, use
`--aws-sigv4 "aws:amz:garage:s3"`. boto3/rclone are easier;
this is for shell scripts.

### Health checks (READ THIS — agents have hallucinated outage reports here)

`https://garagetytus.traylinx.com/` returns **HTTP 403 with a
Garage XML body** to any anonymous request:

```xml
<Error><Code>AccessDenied</Code>
<Message>Forbidden: Garage does not support anonymous access yet</Message>
<Resource>/</Resource><Region>garage</Region></Error>
```

**This response means the endpoint is healthy.** Treat it as a
200 from any other API. Real outages look like:

- TCP connect refused / timeout (`curl --max-time 5` exits 7 or 28).
- `502 Bad Gateway` from Caddy (Garage daemon down behind it).
- boto3 `EndpointConnectionError` / `ConnectTimeoutError`.

Anything else — including `403 AccessDenied`, `403 SignatureDoesNotMatch`,
empty bucket lists, `NoSuchBucket` — is an auth or app issue, not
an endpoint outage. See `docs/MANUAL.md` §17 for the full table.

### What you DON'T need

- WireGuard / `tytus connect` — the public endpoint requires no VPN.
- An AWS account — the access key is a Garage-issued SigV4 key,
  not an AWS IAM key.
- Special CA bundles — Caddy serves a standard Let's Encrypt cert.

### Rate limits

Per-IP soft cap added in a follow-up sprint
(`MAKAKOO/development/sprints/2026-04-26-garagetytus-public-endpoint-followup.md`,
acceptance A3). Today: no rate limiting at the edge. Don't be a
jerk.

### Path-style is mandatory here too

Same as Flavor A —
`config=Config(s3={"addressing_style": "path"})` is required.
Virtual-host style would route the bucket as a subdomain of
`garagetytus.traylinx.com`, which Caddy doesn't have a wildcard
cert for.
