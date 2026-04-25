# garagetytus-shared-folder by Traylinx — pod-side recipe

If you're an AI agent running inside a tytus pod (OpenClaw,
Hermes, or any other pod runtime) and you need to share files
with the user's Mac or with agents in other pods, this is how.

## TL;DR

```bash
. /etc/garagetytus.env
aws s3 cp ./your-file.md s3://shared/from-pod-$(hostname)/$(basename your-file.md) \
    --endpoint $GARAGETYTUS_S3_ENDPOINT \
    --region garage --profile s3-service
```

The user's Mac sees it within seconds (or as soon as the user
runs `rclone bisync` on their shared folder). Other pods see it
immediately via the same bucket.

## Endpoint and credentials

- Endpoint: `http://10.42.42.1:3900` (per-pod stable IP — every
  pod sees the SAME droplet S3 API at this address)
- Bucket: `shared` (the convention; the operator may have
  named it differently — check `/etc/garagetytus.env` or
  ask the user)
- Credentials: in `/etc/garagetytus.env`. Source it once at
  agent startup.

## Three access tools, pick what fits

1. **`aws s3 cp / ls / sync`** — for shell scripts and one-shot
   ops.
2. **`boto3`** — when your agent code is already Python. Use
   `Config(s3={"addressing_style": "path"})` — Garage requires
   path-style addressing, not virtual-hosted-style.
3. **`rclone bisync`** — for keeping a pod-local directory
   mirrored to / from the bucket.

## Naming convention to avoid overwrites

Multiple parties (Mac + N pods) write to the same bucket.

- Files you produce: `from-pod-<your-id>/<descriptive-name>.<ext>`
- Files for everyone: `broadcast/<UTC-iso8601>-<short-hash>.<ext>`
- Files from Mac to you: read from `from-mac/`

## Don't put in the bucket

- Secrets / API keys (use `GARAGETYTUS_HOME` per-pod state)
- Multi-GB blobs without user approval (disk is finite)
- Agent memory / journal entries (Brain owns those, not S3)

See `docs/agents/s3-vs-push-vs-brain.md` for the full decision
tree.

## Watcher pattern (react to user-uploaded files)

```bash
while true; do
    aws s3 sync s3://shared/from-mac/ /app/workspace/inbox/ \
        --endpoint http://10.42.42.1:3900 --profile s3-service
    for f in /app/workspace/inbox/*; do
        [ -e "$f" ] || continue
        # Process $f, then move it out so you don't double-process
        process_message "$f"
        mv "$f" /app/workspace/processed/
    done
    sleep 5
done
```

## Common errors

- `connection refused 10.42.42.1:3900` → Q7 forwarder is dead.
  Tell the user, don't retry blindly. They restart
  `wannolot-network.service` on the droplet.
- `InvalidAccessKeyId` → `/etc/garagetytus.env` is stale. Tell
  the user to re-grant the bucket and refresh the env file.
- `NoSuchBucket` → operator hasn't created the `shared` bucket
  yet. Tell the user to run
  `garagetytus bucket create shared` on the droplet.

## Full reference

`docs/MANUAL.md §12` (Shared folders across Mac + tytus pods)
in this same repo. The Makakoo-style detailed skill at
`skills/garagetytus-shared-folder/SKILL.md` has more depth on
naming conventions, conflict resolution, and the inotify
watcher pattern.

## When NOT to use S3

If the file you're sharing is < 4KB structured text that the
user (or other pods) needs to know about as an event, use the
Brain (`/app/workspace/.brain/journals/<date>.md` append) and
the unified-superbrain syncer instead. S3 is for binary blobs
and large structured data; Brain is for narrative state.
