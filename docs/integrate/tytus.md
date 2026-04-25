# Using garagetytus with tytus

> **Audience:** you have a [tytus](https://traylinx.com/tytus)
> private AI pod (WireGuard tunnel + agent container) and a
> mac-side garagetytus install, and you want the pod's agent to
> read/write S3 buckets that live on your laptop.
>
> **Today (v0.1, Mac host):** works via the existing Makakoo MCP
> shim pattern. No tytus-side changes needed beyond setting one
> environment variable in the pod.
>
> **Phase E (v0.5+, tytus team):** native `tytus bucket *`
> subcommand + tray-menu cell. Spec at the bottom of this doc;
> implementation pending in the separate
> [tytus](https://github.com/traylinx/tytus) repo.

## What tytus is, in two sentences

[tytus](https://traylinx.com/tytus) gives every subscriber an
isolated pod reachable via a userspace WireGuard tunnel. Inside
the pod runs an agent container (OpenClaw + NemoClaw, or Hermes
from Nous Research) backed by SwitchAILocal — an
OpenAI-compatible LLM gateway. The user's laptop talks to the
pod over WG; nothing else does.

## Connection topology when garagetytus enters the picture

```
┌──────────────────────────────────────┐    ┌────────────────────────┐
│ your laptop                          │    │ your tytus pod         │
│                                      │    │                        │
│   garagetytus daemon  127.0.0.1:3900 │    │   agent container      │
│                       127.0.0.1:3903 │    │   (nemoclaw / hermes)  │
│                       127.0.0.1:3904 │    │                        │
│            ▲                         │    │       │                │
│            │ Ed25519 envelope        │    │       │                │
│            │ + SigV4 inner           │    │       │                │
│   ┌────────┴───────────┐             │    │       │                │
│   │ Makakoo MCP shim   │ <wg-ip>:8765│◄───┤───────┘                │
│   │ (signs + forwards) │             │    │                        │
│   └────────────────────┘             │    │   WG tunnel            │
│            ▲                         │    │   (10.42.42.0/16)      │
│            │                         │    │                        │
│   wg0 ◄────┼─────────────── tunnel ──┼────► wg0                    │
└────────────┴─────────────────────────┘    └────────────────────────┘
```

- **garagetytus binds loopback only.** The S3 API at
  `127.0.0.1:3900` is **never** exposed on `0.0.0.0`. Tytus pods
  reach it through the Makakoo MCP shim (port 8765 on the mac's
  WG interface), which authenticates every request with an
  Ed25519 signature before forwarding to garagetytus.
- **Double-signed requests.** SigV4 inner (Garage validates the
  request — your normal AWS signature). Ed25519 outer (the shim
  validates the pod is who it claims to be). Path-style
  addressing mandatory (LD#14).
- **No new infrastructure.** This is the same shim pattern that
  Makakoo's bundled Garage daemon used pre-v0.1. Swapping in
  garagetytus underneath doesn't change the wire protocol.

## Pod-side recipe (what works today)

Inside your tytus pod, anything that speaks S3 can reach the
mac-side garagetytus daemon by setting one environment variable
that points the SDK at the shim:

```bash
# Inside the pod (set by tytus when dispatching, or manually):
export MAKAKOO_PEER_NAME="<your-pod-name>"

# Then either use the bundled Python helper:
python3 -c '
from core.s3 import client     # comes from MAKAKOO/plugins/lib-harvey-core
s3 = client()                  # auto-routes to http://<mac-wg-ip>:8765
print(s3.list_buckets())
'

# …or raw boto3 if you have nothing else available:
python3 -c '
import boto3
from botocore.config import Config
s3 = boto3.client(
    "s3",
    endpoint_url="http://<mac-wg-ip>:8765",
    region_name="garage",
    aws_access_key_id="<from `garagetytus bucket grant` on the mac>",
    aws_secret_access_key="<same>",
    config=Config(s3={"addressing_style": "path"}),
)
print(s3.list_buckets())
'
```

The grant credentials come from the **mac** side. You mint them
once with `garagetytus bucket grant` (or `makakoo bucket grant`
— same thing), then ferry the access/secret pair into the pod
via whatever channel you already use for pod env vars (tytus
config, A2A `/pod/env/set`, or hardcoded for short-lived
experiments).

## Two practical scenarios

### Scenario A — pod produces, mac consumes

A long-running scrape inside the pod writes results to S3; a
Mac-side analysis script reads them.

```bash
# 1. On the mac — create the bucket + a write grant for the pod.
garagetytus bucket create scrape-output --ttl 7d --quota 5G
garagetytus bucket grant scrape-output \
    --to "tytus-scrape-pod" \
    --perms write,list \
    --ttl 24h \
    --json
# →
# {"grant_id": "g_20260425_abcd1234",
#  "access_key": "GK...",
#  "secret_key": "...",
#  "endpoint_url": "http://127.0.0.1:3900",
#  "expires_at": "2026-04-26T17:00:00Z"}

# 2. Send the access/secret pair to the pod (replace mac-local
#    endpoint with the WG-routed shim endpoint).
tytus exec --pod 02 -- bash -c 'cat >/app/workspace/.env <<EOF
S3_ACCESS_KEY=GK...
S3_SECRET_KEY=...
S3_ENDPOINT=http://10.42.42.X:8765   # mac side WG IP + shim port
S3_BUCKET=scrape-output
EOF'

# 3. The pod's scraper writes to s3://scrape-output/...

# 4. Mac side reads back via the local endpoint.
aws s3 cp s3://scrape-output/2026-04-25/ ./out/ --recursive \
    --endpoint-url http://127.0.0.1:3900 \
    --no-verify-ssl
```

### Scenario B — mac stages, pod reads

The mac drops a model checkpoint or a dataset into a bucket; a
training run inside the pod consumes it.

```bash
# 1. Mac side — create bucket, upload, mint a read grant.
garagetytus bucket create model-checkpoints --ttl permanent --quota 50G
aws s3 cp ./checkpoint-final.bin s3://model-checkpoints/ \
    --endpoint-url http://127.0.0.1:3900

garagetytus bucket grant model-checkpoints \
    --to "tytus-trainer-pod" \
    --perms read,list \
    --ttl 4h \
    --json

# 2. Hand the access/secret + the shim URL to the pod.
# 3. Pod reads via http://<mac-wg-ip>:8765 (same boto3 config
#    pattern as Scenario A, with read-only creds).

# 4. When the run finishes:
garagetytus bucket revoke g_20260425_abcd1234
```

## Why route through the shim and not WG-direct

You **could** in principle bind garagetytus to the mac's WG
address (`10.42.42.X:3900`) and let the pod hit that directly.
Don't. Three reasons:

1. **Auth.** The mac-side keychain holds the SigV4 secret. If
   you expose the S3 API on the WG address without the Ed25519
   shim, every pod that ever joins the WG sees the bare SigV4
   surface. The shim's Ed25519 layer is per-pod-keyed, so
   revoking a pod is one trust-store edit.
2. **Audit.** The shim writes to Makakoo's audit log every
   forwarded request. WG-direct skips that.
3. **Per-grant scoping.** The shim resolves a pod's identity
   first, then maps to the per-pod set of allowed grants. The
   bare S3 surface trusts whoever holds the SigV4 secret.

LD#11 codifies this: garagetytus binds `127.0.0.1`. Period.

## What about big files?

The shim caps inline-forwarded bodies at **10 MB** (configurable
in the shim, but 10 MB is the default). For larger objects, mint
a presigned PUT URL on the mac and have the pod upload to that
URL directly:

```python
# Mac side:
url = s3.generate_presigned_url(
    "put_object",
    Params={"Bucket": "model-checkpoints", "Key": "epoch-99.bin"},
    ExpiresIn=600,
)
# Send `url` to the pod.

# Pod side:
import requests
with open("/scratch/epoch-99.bin", "rb") as f:
    requests.put(url, data=f)
```

The presigned URL is just a SigV4-signed pointer. The pod's
Ed25519 wrapper **does** still need to apply on the actual
upload because the URL targets the shim address — the URL
carries the signature, but the transport layer still needs the
peer envelope. See `MAKAKOO/plugins/skill-s3-endpoint/SKILL.md
§"Object size ceiling"` for the exact wrapper.

## Phase E — native `tytus bucket *` (spec; pending)

The tytus team's roadmap (sprint Phase E) adds two surfaces that
delegate to garagetytus, so users don't have to construct shim
URLs by hand:

### Subcommand contract

`tytus bucket *` would forward args to `garagetytus bucket *`
verbatim, resolving the right endpoint per caller context (mac
vs pod) the way `core.s3.CallerContext.from_runtime()` does
today. Mac-local invocations exec `garagetytus bucket *`
directly with inherited stdio (the Q2 Makakoo wrapper pattern).
Pod-side invocations route through the existing tytus tunnel +
Ed25519 envelope to the mac-side shim at `<wg-ip>:8765`.

### Flag matrix

| `tytus bucket ...` | What it expands to |
|---|---|
| `create my-data --ttl 7d --quota 10G` | `garagetytus bucket create my-data --ttl 7d --quota 10G` |
| `ls` | `garagetytus bucket list --json` (pretty-printed by tytus) |
| `push <local> s3://my-data/<key>` | `bucket grant ... --ttl 5m` → `aws s3 cp` → `bucket revoke` |
| `pull s3://my-data/<key> <local>` | mirror of push |
| `grant my-data --to <label> [--ttl] [--perms]` | `garagetytus bucket grant ...` (passthrough) |
| `revoke <grant-id>` | `garagetytus bucket revoke <grant-id>` |

`push` / `pull` add value over manually `grant + cp + revoke`
by chaining the three into a single command with a short-lived
TTL.

### Tray-menu cell contract

```
🪣 Buckets — N total, M.MM GB / Q GB                (status: OK / RO)
   ├─ Open in Filebrowser                              [opens browser]
   ├─ Show recent grants…                              [submenu]
   └─ Garagetytus version: 0.1                         [click → repo]
```

Numbers from `garagetytus capabilities --json`. OK/RO badge from
the LD#11 watchdog protocol — reads `<state-dir>/watchdog.json`
and surfaces the `mode = "rw"` / `"ro"` flag. Cells refresh
every 30 s; the tray daemon polls the JSON surfaces, no IPC
socket required.

### Error contract

When the mac doesn't have garagetytus installed, `tytus bucket *`
MUST surface the same fallback message Makakoo uses:

```
garagetytus not found — install at https://garagetytus.dev
```

…on stderr, exit non-zero. **No silent fallback to a
tytus-embedded Garage path** (LD#1).

### Implementer pointers (for the tytus team)

- `makakoo-os/makakoo/src/commands/bucket.rs` (commit `ae97464`)
  — reference wrapper. Phase D Option A pattern. Inherit stdio,
  don't capture-and-relay. ~250 LOC, eight unit tests.
- `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/verdicts/Q2-VERDICT.md`
  — full lope verdict (pi+codex PASS Option A). The tytus side
  doesn't need its own lope round; the patterns are pre-locked.
- `MAKAKOO/plugins/skill-s3-endpoint/SKILL.md` — the existing
  pod-side boto3 + Ed25519 wrapper pattern. Phase E reuses it
  verbatim.
- garagetytus side requires **zero** tytus-aware code (LD#12
  forbids the inverse coupling). Spec changes here are the only
  delivery on this repo.

### v0.1 → v0.2 outlook

- v0.1 ships **mac-only** for tytus-side reasons (the `tytus`
  binary is mac-only Mach-O today). When tytus-cli ships
  Linux/Windows, the adapter widens automatically — `tytus
  bucket` is platform-agnostic Rust orchestration around a
  platform-agnostic `garagetytus` child.
- v0.2 considerations: pod-side `bucket push/pull` chaining,
  multi-pod buckets (one bucket reachable from N pods at once
  with per-pod scoped grants).

## Until Phase E ships

Use the pod-side recipe at the top of this doc — it works today
with zero tytus-side changes. The mac mints grants via
`garagetytus bucket grant ... --json`, you ferry the
access/secret to the pod, and the pod talks to the shim address
just like every other Makakoo-aware pod-side workload.

---

## Folder ↔ bucket sync (when you want a "shared folder" UX)

Three tiers, smallest to biggest, depending on how seriously you
want offline / pod-to-pod behavior. **None of these are
garagetytus's job** — garagetytus is the S3 wire (buckets +
grants + daemon). The "folder ↔ bucket" relationship is a
sync layer on top.

### Tier 1 — rclone in the pod (works today, zero new code)

For a single pod that wants `/app/workspace/data/` to stay
in sync with `s3://my-bucket/work/`:

```bash
# Mac side, once: create the shared bucket + a long-lived grant.
garagetytus bucket create pod-workspace --ttl permanent --quota 10G
garagetytus bucket grant pod-workspace --to "tytus-pod-sync" \
    --perms read,write,list --ttl permanent --json
# → access_key, secret_key

# Pod side, once: configure rclone with the shim endpoint.
cat > ~/.config/rclone/rclone.conf <<EOF
[mac-garage]
type = s3
provider = Other
endpoint = http://<mac-wg-ip>:8765
region = garage
access_key_id = GK...
secret_access_key = ...
force_path_style = true
EOF

# Pod side, recurring: bidirectional sync every 60s.
* * * * * rclone bisync /app/workspace/data mac-garage:pod-workspace/work \
    --resync-on-first-run --conflict-resolve newer \
    --log-file /tmp/rclone-bisync.log
```

When the mac is reachable: changes flow both ways. When the
mac is unreachable: rclone errors, the cron tick skips, local
writes accumulate. Next time mac comes back, the queued changes
flush. Conflicts fall through to rclone's resolver
(`--conflict-resolve newer` = last-writer-wins by mtime;
`--conflict-resolve none` keeps both as `.conflict-<ts>` files).

**Constraints to internalize before reaching for Tier 1:**

- **Bandwidth.** Initial sync of large data over WG is slow.
  Be explicit about the upfront cost.
- **Eventual consistency.** Pod writes aren't visible to the
  mac until the next sync tick. Don't position this as "shared
  filesystem"; it's "shared eventually-consistent S3 bucket
  with a sync helper."
- **Pod disk cost.** A 10 GB bucket needs 10 GB of pod disk.
  Pod plans need to budget for this.
- **Mac-off scenarios.** When the mac is closed for hours, the
  pod's local cache is the source of truth. Mac comes back, the
  merge window can be large.

### Tier 2 — pod-to-pod sync band-aid (caveat-heavy)

If you have **two pods** and both want to share state when the
mac is off, you can add a peer-to-peer rclone bisync between
them — pods are on the same WG /16 so they can route to each
other. Each pod sees three sync targets (mac, pod-A, pod-B),
rclone reconciles whatever's reachable.

**This works, but it's a band-aid.** Three-way conflicts when
both pods edit while mac is off. Sync ordering drift — pod 1
might learn about pod 2's change via mac before via direct, or
vice versa. No transactional guarantees. Acceptable for shared
notes; bad for shared model checkpoints or anything where
ordering matters.

For the rclone config + cron timer setup, the pattern is the
same as Tier 1 — just point each pod's rclone at TWO remotes
(mac-garage + peer-pod-NN) and run bisync against both
periodically. The peer-pod target uses the same shim host:port
shape, but a different mac account (you mint a separate grant
per peer for audit clarity).

If you find yourself wanting Tier 2, **that's the trigger** for
Tier 3 — please skip the band-aid and queue the v0.5 multinode
sprint instead. Tier 3 solves what Tier 2 papers over.

### Tier 3 — multi-node Garage cluster (real fix; v0.5 sprint)

The architecturally correct answer to "mac off, pods still
work, pods can share state": run a **2-node Garage cluster**
spanning the Mac and the Tytus control-plane droplet. Pods
route to the droplet directly (`http://10.42.42.1:3900/`).
When the Mac is off, the droplet keeps serving. When the Mac
is on, Garage anti-entropy reconciles. Pure Garage native
replication; we just wrap the bootstrap.

**Topology:**

```
mac (zone="mac")  ◄──── cluster RPC (port 3901, WG) ────►  droplet (zone="droplet")
        │                                                          │
        │ 127.0.0.1:3900 (S3 API, mac-local)                       │ 0.0.0.0:3900 (S3 API, WG-bound, iptables-restricted to 10.42.42.0/24)
        │                                                          │
        └─── pod 1 (read+write via shim :8765) ──── pod 2 (read+write via :3900 direct) ┘
                            ▲                                         ▲
                            └── direct route to droplet over WG ──────┘
```

When mac is up: pods can use either path (shim or direct).
When mac is off: pods use the direct path. Pod 1 writes →
droplet replicates → pod 2 reads. **No pod-local Garage** —
explicitly forbidden by the canonical sprint's LD#11 (would
create N+1 cluster members, explode replication cost, couple
data survival to pod lifecycle).

**Why pod-local would be the wrong answer here.** N pods means
N+1 cluster nodes; every write replicates N+1 times; pod
shutdown takes data with it unless persistent volumes are
mounted (which Tytus pods don't do today). Routing pods through
the always-on droplet is cheaper, simpler, and survives pod
churn for free.

**Status: drafted + lope-hardened, not yet executed.**

| Doc | Where |
|---|---|
| Wrapper sprint with carve-out deltas | `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/SPRINT.md` |
| Canonical 802-LOC spec (pi + qwen lope round 1 applied) | `MAKAKOO/development/sprints/queued/MAKAKOO-OS-V0.8-S3-CLUSTER/SPRINT.md` |
| pi verdict (~11 fixes, binding-address bug among them) | `MAKAKOO-OS-V0.8-S3-CLUSTER/verdicts/LOPE-VERDICT-PI.md` |
| qwen verdict (~13 fixes incl. ListObjects retry, presigned URL race) | `MAKAKOO-OS-V0.8-S3-CLUSTER/verdicts/LOPE-VERDICT-QWEN.md` |

**Trigger to run the sprint.** Sebastian needs pod-pod sharing
or always-on bucket access for a real workflow (not just a
what-if). Until then, Tier 1 (mac-side single-node + rclone) is
enough for most dev workflows.

**Rough cadence.** ~9 days, one coder. Phase 0 probes (3 hours)
gate everything else. Three new garagetytus-side design
questions (Q4 cluster-init invocation host, Q5 multi-node
auto-repair, Q6 cluster-wide mode derivation) need a fresh
lope round before Phase A.

### Decision matrix

| Need | Reach for |
|---|---|
| One pod, mac usually on, dev workflow | Tier 1 (rclone) |
| One pod, mac sometimes off for hours | Tier 1 (rclone) — local cache stays available read-only |
| Two pods, mac always on, mostly read-shared | Tier 1 (rclone) on each pod, both targeting mac |
| Two pods, mac sometimes off, occasional pod-pod writes | Tier 2 (rclone P2P) — accept the conflict caveats |
| Two pods, real-time shared state, mac sometimes off | Tier 3 (v0.5 multinode sprint) — queue the sprint |
| Production-grade always-on bucket access | Tier 3 |

**Tier 3 is the goal for "real" pod-pod sharing.** Tiers 1 and
2 are valid stops on the way there.

## Source pointers

| File | What it's for |
|---|---|
| `MAKAKOO/plugins/skill-s3-endpoint/SKILL.md` | Pod-side boto3 + Ed25519 wrapper pattern (the canonical reference) |
| `makakoo-os/makakoo-mcp/` | The Mac-side MCP shim binding `<wg-ip>:8765` |
| `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/SPRINT.md §Phase E` | Full Phase E spec the tytus team codes against |
| `MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.1/verdicts/Q2-VERDICT.md` | Wrapper-pattern verdict (apply verbatim to tytus side) |

## Q&A

**Q: Can I run garagetytus inside a tytus pod instead of on the
mac?**
A: Technically yes (it's just a Rust binary), but the design
expects mac-side. The mac's keychain holds the service identity;
the pod has no equivalent keychain (containers are ephemeral).
If you need an in-pod object store, run a separate garagetytus
inside the pod with `GARAGETYTUS_HOME=/app/workspace/garagetytus`
— but you'll have to manage its keychain alternative manually.
Out of scope for v0.1 docs.

**Q: My pod isn't on Sebastian's tytus install — does this
still work?**
A: The pod-side recipe needs **two** environment pieces: (1) the
shim URL is reachable (means: the WG tunnel is up to a host
running Makakoo + garagetytus), (2) `MAKAKOO_PEER_NAME` is set
so the helper picks the shim path. If your pod is on a different
deployment, point `endpoint_url` at whatever shim/proxy that
deployment exposes. The wire protocol is standard SigV4 — only
the transport wrapper differs.

**Q: Does garagetytus run inside the tytus droplet (server
side)?**
A: No — the user-facing tytus product runs the agent containers
inside the droplet, but garagetytus is **mac-side only** in v0.1.
There is no server-side garagetytus deployment by design (LD#12
— garagetytus knows nothing about pods, droplets, or
WireGuard).
