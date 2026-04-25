# Grants — per-bucket sub-keypair flow

The garagetytus grant system lets you mint short-lived,
per-bucket, per-consumer S3 keypairs without giving the consumer
your bootstrapped service credentials.

> Inherited from Makakoo v0.7.1 (locked schema version 1).
> Same on-disk format; same Python + Rust drift fixtures. The
> `~/.garagetytus/grants.json` file is gitignored, machine-local,
> never synced.

## Why grants

Bare boto3 with the bootstrapped `s3-service` keypair gives a
caller full read+write+admin across every bucket on this host.
That's fine for personal use; it's not fine for:

- An external app you've installed but don't fully trust.
- A subprocess you're delegating a single transfer to.
- A docker container that should only see one bucket for one
  hour.

A grant ties:
- a label (human-readable, audit-friendly),
- a bucket,
- a permission set (`read | read,write | read,write,owner`),
- a TTL (`30m | 1h | 24h | 7d | permanent`),
- a sub-keypair (provisioned by Garage's `key create` API),
- a `grant_id` (`g_<yyyymmdd>_<8hex>`).

## Mint, use, revoke

```bash
# Mint
GRANT=$(garagetytus bucket grant my-data \
        --to "my-app" --perms read,write --ttl 1h --json)
ACCESS=$(echo "$GRANT" | jq -r .access_key)
SECRET=$(echo "$GRANT" | jq -r .secret_key)

# Use — boto3 / aws-cli / rclone / whatever speaks S3.
aws --endpoint-url http://127.0.0.1:3900 \
    --access-key-id "$ACCESS" \
    --secret-access-key "$SECRET" \
    s3 cp /etc/hostname s3://my-data/hostname

# Revoke when done (or wait for TTL).
garagetytus bucket revoke $(echo "$GRANT" | jq -r .grant_id)
```

## Permanent grants

Use `--ttl permanent` with `--confirm-yes-really`. The grant
never auto-expires; it's revocable only via `bucket revoke`.

Permanent grants go in the audit log with a special
`permanent_yes_really` tag.

## Rate limits

Hard caps (LD#7 from the v0.3 spec):
- 20 active grants total per host.
- 50 grant-create operations per rolling hour.

Exceeding either → `rate limit: N active grants; revoke some or
wait` (verbatim error string per Makakoo Q-1 verdict).

The rate-limit counter lives in `state/perms_rate_limit.json` —
**a separate file from grants.json** so a corrupt counter cannot
poison the grant store (Lope F7).

## Audit log

Every grant create / revoke / use writes one JSON line to
`~/.garagetytus/logs/audit.jsonl`. Rotation kicks in at 100 MB.
Untrusted fields (labels, scopes) are escape-encoded via
`escape_audit_field` so log lines stay parseable when the
caller injects control chars (LD#16).

## Schema (version 1)

```json
{
  "version": 1,
  "grants": [
    {
      "id": "g_20260425_a3f9c12d",
      "scope": "s3/bucket:my-data",
      "created_at": "2026-04-25T12:42:00Z",
      "expires_at": "2026-04-25T13:42:00Z",
      "label": "my-app",
      "granted_by": "sebastian",
      "plugin": "cli",
      "origin_turn_id": "",
      "owner": "cli"
    }
  ]
}
```

Mutating the schema requires coordinated updates across:
- `crates/garagetytus-grants/src/user_grants.rs` (Rust)
- `sdk/python/src/garagetytus/__init__.py` (Python)
- The shared drift fixtures in `tests/fixtures/`.

The fixtures fail in lockstep on drift — that's the gate.

## Cross-product visibility

Makakoo + tytus consume `~/.garagetytus/grants.json` **read-only**
via the same on-disk format (LD#9). A grant minted by
`makakoo bucket grant my-data --to "downstream-pod"` is visible
to `garagetytus bucket info my-data` without restart, and vice
versa.
