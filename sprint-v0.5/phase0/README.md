# Phase 0 probes — GARAGETYTUS-V0.5-MULTINODE

The 8 probes that gate Phase A. Sequential; any failure pauses the
sprint for diagnosis.

> **You run these.** They need real droplet SSH access + (for
> P0.2) a local docker host. Cannot run from a chat session.

## One-shot

```bash
bash sprint-v0.5/phase0/probe.sh user@your-droplet-host
```

Captures output to `sprint-v0.5/phase0/results/PHASE-0-RESULTS-<date>.md`.

After all 8 finish, copy the results doc to:

```
MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/results/PHASE-0-RESULTS.md
```

## Per-probe scripts

| Probe | Time | Script | What it locks |
|---|---|---|---|
| P0.1 | 15 min | `p01-droplet-access.sh` | LD#9 — Linux x86_64, ≥20 GB free |
| P0.2 | 60 min | `p02-multi-node-probe.sh` | LD#3 + LD#12 — replication + SQLite + partition |
| P0.3 | 30 min | `p03-rpc-probe.sh` | LD#3 — RPC nature + WG-RTT tolerance |
| P0.4 | 30 min | `p04-wg-throughput.sh` | landmines — large-object replication budget |
| P0.5 | 15 min | `p05-pod-reachability.sh` | LD#4 — pod → droplet WG IP works |
| P0.6 | 15 min | `p06-secret-channel.sh` | LD#5 — SSH-based secret transport |
| P0.7 | 20 min | `p07-deny-all.sh` | LD#7 + Phase C.3 — deny-all mechanism |
| P0.8 | 15 min | `p08-binary-version.sh` | LD#16 — version + SHA pin |

Total: 3 hours sequential.

## Gate

Phase A starts only after every probe records an outcome and
no acceptance-gate flag is unresolved. P0.3 + P0.5 + P0.7 are
the load-bearing probes — outcomes directly shape Phase A
implementation.

## Re-run policy

Probes are non-destructive (P0.6 cleans its own probe file; P0.7
uses test buckets the operator names). Re-running is safe; cherry-
picking a single probe is also safe (call the per-script directly
with the host arg).
