---
name: garagetytus-shared-folder
version: 0.1.0
description: |
  How agents inside a tytus pod (or any S3 client — Mac, Linux,
  remote box) read and write files in a shared bucket on the
  droplet so they can collaborate with the user's Mac and with
  other pods. Covers credential discovery, the S3 endpoint, the
  three common access tools (aws CLI / boto3 / rclone), the
  conflict-free naming convention, and the inotify watcher
  pattern for "pick up new files automatically." Use this when
  an agent needs to drop a file the user (or another pod) will
  see — or read a file the user just added.
allowed-tools:
  - Bash
  - Read
  - Write
category: collaboration
tags:
  - garagetytus
  - s3
  - shared-folder
  - tytus
  - pods
  - rclone
  - boto3
  - aws-cli
---

# garagetytus-shared-folder — pod-side recipe for sharing files with Mac and other pods

This skill tells you, an agent running inside a tytus pod (or
any environment with WireGuard access to the shared S3 endpoint),
how to share files with the user and with other agents through
the bucket on the droplet.

## Mental model

```
   [your pod]            [droplet bucket]            [Mac + other pods]
        │                       │                            │
        │   PUT s3://shared/X   │                            │
        ├──────────────────────►│                            │
        │                       │      GET s3://shared/X     │
        │                       │◄───────────────────────────┤
        │                       │                            │
```

The droplet runs a single-node Garage S3 daemon. Mac mounts the
bucket via `rclone bisync` to a local folder. Pods access the
bucket directly via the S3 API. Anything written from any party
becomes visible to all parties immediately.

## Quick recipe — drop a file the user will see

```bash
echo "Hello Sebastian, I generated this report at $(date)" \
    > /tmp/report-$(date +%s).md

aws s3 cp /tmp/report-$(date +%s).md s3://shared/ \
    --endpoint http://10.42.42.1:3900 \
    --region garage \
    --profile s3-service
```

Within seconds (or after the user runs `rclone bisync` in their
shared folder), the file appears in `~/Documents/shared-with-pods/`
on the user's Mac.

## Endpoint + credentials

The S3 endpoint is **always `http://10.42.42.1:3900`** from inside
any pod that has the `setup-pod-proxy.sh` Q7 forwarder enabled
(every wannolot-NN sidecar does). HTTPS is not needed because the
WireGuard tunnel encrypts the layer below.

Credentials live in `/etc/garagetytus.env`:

```
GARAGETYTUS_S3_ENDPOINT=http://10.42.42.1:3900
GARAGETYTUS_AWS_ACCESS_KEY_ID=GK…
GARAGETYTUS_AWS_SECRET_ACCESS_KEY=…
GARAGETYTUS_S3_REGION=garage
```

Source it once:

```bash
. /etc/garagetytus.env

aws configure set aws_access_key_id "$GARAGETYTUS_AWS_ACCESS_KEY_ID" \
    --profile s3-service
aws configure set aws_secret_access_key "$GARAGETYTUS_AWS_SECRET_ACCESS_KEY" \
    --profile s3-service
aws configure set region garage --profile s3-service
```

(If the env file isn't there yet, ask the user to run
`makakoo bucket grant shared --to <pod-name>` on Mac and paste
the output into `/etc/garagetytus.env`. v0.5.1 will automate this
fetch over SSH.)

## The three common access patterns

### 1. aws CLI — one-shot PUT or GET

```bash
# Push a file
aws s3 cp ./output.json s3://shared/from-pod-02/output.json \
    --endpoint http://10.42.42.1:3900 --profile s3-service

# Pull a file
aws s3 cp s3://shared/from-mac/instructions.md ./ \
    --endpoint http://10.42.42.1:3900 --profile s3-service

# List
aws s3 ls s3://shared/ \
    --endpoint http://10.42.42.1:3900 --profile s3-service
```

Use this for ad-hoc transfers from a shell command.

### 2. boto3 — programmatic from Python

```python
import boto3, os
from botocore.config import Config

s3 = boto3.client(
    "s3",
    endpoint_url="http://10.42.42.1:3900",
    aws_access_key_id=os.environ["GARAGETYTUS_AWS_ACCESS_KEY_ID"],
    aws_secret_access_key=os.environ["GARAGETYTUS_AWS_SECRET_ACCESS_KEY"],
    region_name="garage",
    config=Config(s3={"addressing_style": "path"}),
)

# PUT
s3.put_object(Bucket="shared", Key="from-pod-02/result.json",
              Body=b'{"ok": true}', ContentType="application/json")

# GET
data = s3.get_object(Bucket="shared", Key="from-mac/input.md")["Body"].read()
```

Use this when your agent code is already Python — no shellout
overhead, structured errors.

### 3. rclone — folder-level mirroring

If you want to mirror a whole pod-local directory to / from the
bucket:

```bash
# One-time bootstrap of bisync state:
rclone --config /etc/rclone.conf bisync /app/workspace/shared garagetytus:shared --resync

# Subsequent two-way sync (run from cron / inotifywait):
rclone --config /etc/rclone.conf bisync /app/workspace/shared garagetytus:shared
```

`/etc/rclone.conf`:

```ini
[garagetytus]
type = s3
provider = Other
access_key_id = $GARAGETYTUS_AWS_ACCESS_KEY_ID
secret_access_key = $GARAGETYTUS_AWS_SECRET_ACCESS_KEY
endpoint = http://10.42.42.1:3900
region = garage
```

## Conflict-free naming convention

Multiple pods + Mac all write to the same bucket. To avoid
overwrites:

| Source | Convention | Example |
|---|---|---|
| Mac | `from-mac/<filename>` | `from-mac/instructions.md` |
| Pod NN | `from-pod-NN/<filename>` | `from-pod-02/result.json` |
| Cross-pod broadcast | `broadcast/<utc-iso8601>-<sha256-prefix>.<ext>` | `broadcast/2026-04-26T12:00:00Z-a3f9c1.md` |

When you write a result that another agent will react to, **use a
unique key including a timestamp or content hash**. Don't
overwrite existing keys unless you own them.

## React to new files (inotify-style)

For "pick up files the user (or another pod) added":

```bash
while true; do
    aws s3 sync s3://shared/from-mac/ /app/workspace/inbox/ \
        --endpoint http://10.42.42.1:3900 --profile s3-service \
        --exclude "*" --include "*.md"
    # Process new files in /app/workspace/inbox/ here
    for f in /app/workspace/inbox/*.md; do
        [ -e "$f" ] || continue
        process_message "$f"
        mv "$f" /app/workspace/processed/
    done
    sleep 5
done
```

For lower latency, pair `aws s3 sync` with bucket polling on
event hashes (or use rclone's `--check-access` mode).

## Don't put these in the shared bucket

- **Secrets, API keys, passwords.** The bucket is shared with
  every pod the user has provisioned. Use the per-pod
  `GARAGETYTUS_HOME` data dir for per-pod secrets.
- **Multi-GB blobs** without explicit user approval. The droplet
  has finite disk; pods that hammer the bucket get throttled.
- **Conversational history / agent memory.** Put that in the
  Brain (`/app/workspace/.brain/journals/`) — Brain is the
  narrative substrate, S3 is the binary substrate. See
  `docs/agents/s3-vs-push-vs-brain.md` for the decision tree.

## When the user asks "do you see X file?"

Always check the bucket fresh — don't rely on a cached list.
Files may have been added by Mac or another pod since the last
sync.

```bash
aws s3 ls s3://shared/ --recursive \
    --endpoint http://10.42.42.1:3900 --profile s3-service \
    | tail -20
```

## When something fails

| Symptom | Likely cause | Fix |
|---|---|---|
| `An error occurred (NoSuchBucket)` | bucket name typo, or operator hasn't created `shared` | `ssh root@<droplet> "garagetytus bucket create shared"` from Mac |
| `An error occurred (InvalidAccessKeyId)` | `/etc/garagetytus.env` missing or stale | Ask user to re-run `garagetytus bucket grant shared --to <pod>` and refresh env |
| `dial tcp 10.42.42.1:3900: connection refused` | Q7 socat forwarder dead, OR Garage daemon down | Operator side: `ssh root@<droplet> "systemctl restart wannolot-network garagetytus"` |
| `An error occurred (ServiceUnavailable): Could not reach quorum` | Bucket has rf=2 but cluster only has 1 healthy node — known v0.5 limit | Operator reduces `replication_factor` to 1 in droplet config and resets layout |

## Skill done — what to do next

When the user explicitly asks to:

- **"share a file with my Mac"** → use `aws s3 cp` with `from-pod-NN/` prefix.
- **"check if the user sent me anything"** → `aws s3 ls s3://shared/from-mac/`.
- **"set up automatic sync of /app/workspace/shared"** → write the rclone bisync config + a cron entry.
- **"why can't I see X"** → run the troubleshooting matrix above.

This skill is paired with the Mac-side `garagetytus-daily-ops`
skill — when a Mac-side agent asks "what bucket does my pod use",
that skill points back here.
