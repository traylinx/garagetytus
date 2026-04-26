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
how to share files with the user and with agents in other pods
through buckets on the droplet.

## v0.5.1 default path: use `garagetytus-shared`

If `/app/workspace/.garagetytus/garagetytus-shared` exists in
your pod, that is the **only** thing you need. The Mac-side
`garagetytus folder bind` flow has already minted your pod's
credentials, dropped them at
`/app/workspace/.garagetytus/credentials.json`, and appended a
fragment to your system prompt. Use the wrapper:

```bash
garagetytus-shared buckets                            # what you have access to
garagetytus-shared whoami                             # your pod identity + audit
garagetytus-shared list                               # default bucket
garagetytus-shared list --bucket work --prefix from-mac/
garagetytus-shared put ./report.md                    # auto-keyed: from-<pod-id>/report.md
garagetytus-shared put ./report.md --key meeting/2026-04-26.md
garagetytus-shared get from-mac/agenda.md /tmp/agenda.md
garagetytus-shared rm from-<pod-id>/old-file.md --yes
```

The wrapper picks the right bucket if you omit `--bucket` (the
default-marked bucket in your credentials), uses path-style
addressing, retries up to 3x, and prints clear error messages
if endpoint or credentials fail. **You never see access keys.**

If the wrapper isn't installed, ask Sebastian to run on his Mac:

```bash
garagetytus folder bind ~/Documents/<topic> <bucket> --to <your-pod-id>
```

That sets up everything. Then come back here.

The rest of this skill — the `aws CLI`, `boto3`, `/etc/garagetytus.env`
recipes — describes the **v0.5.0 manual fallback path**. Use those
only when the wrapper genuinely doesn't exist (very old pod images
or non-tytus environments). For all normal work in v0.5.1+, the
wrapper above is the answer.

---

## Mental model — one bucket = one shared folder

```
   [your pod]                  [droplet]                    [Mac]
        │                          │                          │
        │  PUT s3://work/X         │  bucket: work            │
        ├─────────────────────────►│  rclone bisync ↔         │
        │                          │  ~/Documents/work        │
        │                          ├─────────────────────────►│
        │                          │                          │
        │  PUT s3://personal/Y     │  bucket: personal        │
        ├─────────────────────────►│  rclone bisync ↔         │
        │                          │  ~/Documents/personal    │
        │                          ├─────────────────────────►│
```

**Each shared folder is a SEPARATE BUCKET.** The bucket name IS
the folder identity. Sebastian creates a bucket per topic
(`work`, `personal`, `agent-results`, etc.) and grants each one
to specific pods. Your pod sees a bucket only if it has been
granted access. Different files in the wrong bucket = wrong
audience seeing them — when in doubt, **ask Sebastian which
bucket to write to**.

## Step 0 — discover which buckets your pod can use

```bash
. /etc/garagetytus.env

# Preferred: read the JSON manifest if present —
# it lists every granted bucket plus its intended local mount path:
test -f /etc/garagetytus.shared.json && cat /etc/garagetytus.shared.json
# Example output:
# {
#   "buckets": [
#     {"name": "work",     "mount": "/app/workspace/shared/work",     "perms": "rw"},
#     {"name": "personal", "mount": "/app/workspace/shared/personal", "perms": "ro"},
#     {"name": "results",  "mount": "/app/workspace/shared/results",  "perms": "rw"}
#   ]
# }

# Fallback: enumerate via the S3 API
aws s3api list-buckets \
    --endpoint $GARAGETYTUS_S3_ENDPOINT \
    --profile s3-service
```

If only one bucket is granted, the rest of this skill defaults
the bucket name to `shared` — substitute yours where it appears.

## Picking the right bucket for the task

| Task | Bucket |
|---|---|
| Drop a work-related file the user will see in `~/Documents/work` | `work` |
| Hand off a result to another agent | `agent-results` (or whatever Sebastian uses) |
| Broadcast to all pods | `broadcast` if granted, otherwise the all-pods bucket Sebastian named |
| Unsure | Ask Sebastian. Don't guess. |

## Quick recipe — drop a file the user will see

```bash
. /etc/garagetytus.env
BUCKET=work        # ← the bucket Sebastian wants this kind of file in
POD=$(hostname)    # ← e.g. "wannolot-02" → tells the user which pod produced it

aws s3 cp ./report.md s3://$BUCKET/from-$POD/report-$(date +%s).md \
    --endpoint $GARAGETYTUS_S3_ENDPOINT \
    --region garage --profile s3-service
```

Within seconds (or as soon as Sebastian's `rclone bisync`
runs against that bucket), the file appears in
`~/Documents/$BUCKET/from-$POD/report-….md` on Mac.

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
