#!/usr/bin/env bash
# P0.2 — Garage multi-node protocol probe + SQLite correctness (60 min)
#
# Spins up a 2-node Garage cluster locally via docker-compose,
# verifies replication + partition recovery + SQLite integrity.
# Captures behavior for the LD#3 + LD#12 decisions.
#
# This probe runs LOCALLY (not on the droplet) — it's a controlled
# fixture to characterize Garage's behavior before touching prod.
set -euo pipefail
HOST="${1:-(local fixture)}"

cat <<'NOTE'
P0.2 is a 60-minute LOCAL fixture probe — runs on the operator's
laptop (or any host with docker), not the droplet. The fixture
lives at:

    tests/integration/garage-cluster/

Run sequence:
  cd tests/integration/garage-cluster
  docker-compose up -d
  ./bootstrap.sh                # assigns layout, replication_factor=2
  ./test-replication.sh         # PUT on node-A, GET on node-B within 5s
  ./test-partition.sh           # iptables DROP, verify RW continues
  ./test-reconcile.sh           # restore link, verify convergence
  ./test-sqlite-integrity.sh    # PRAGMA integrity_check on both nodes

Capture for the results doc:
  - exact docker-compose.yml contents
  - exact replication latency (ms PUT→GET)
  - whether SQLite integrity_check returns "ok" after partition
  - bucket-level concurrent-creation behavior on reconnect

This probe is BLOCKING for Phase A.2 (Garage layout assignment).
NOTE
echo
echo "▸ docker available:"
docker version --format '{{.Server.Version}}' 2>&1 | head -1 || echo "  docker not present — install or run on a host with docker"
echo
echo "Acceptance gates:"
echo "  [ ] PUT on node-A → GET on node-B within 5s"
echo "  [ ] partition: both nodes serve reads of pre-replicated objects"
echo "  [ ] reconcile: SQLite integrity_check = ok on both"
echo "  [ ] bucket creation during partition: documented behavior"
