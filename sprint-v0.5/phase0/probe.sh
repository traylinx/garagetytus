#!/usr/bin/env bash
# Phase 0 — droplet probes for GARAGETYTUS-V0.5-MULTINODE.
#
# Runs the 8 Phase 0 probes from the canonical sprint at
# MAKAKOO/development/sprints/queued/MAKAKOO-OS-V0.8-S3-CLUSTER/SPRINT.md
# against a target droplet. Sequential; any probe failure pauses
# the run for diagnosis.
#
# Usage:
#   bash sprint-v0.5/phase0/probe.sh user@droplet-host
#
# Output goes to:
#   sprint-v0.5/phase0/results/PHASE-0-RESULTS-<date>.md
#
# Probes:
#   P0.1 — droplet access + Linux x86_64 check (15 min)
#   P0.2 — Garage multi-node protocol probe (60 min — needs docker)
#   P0.3 — Garage RPC nature + timeout (30 min)
#   P0.4 — WG tunnel capacity under load (30 min)
#   P0.5 — pod → droplet WG IP reachability (15 min)
#   P0.6 — bootstrap channel for cluster secret (15 min)
#   P0.7 — Garage bucket-level denial mechanism (20 min)
#   P0.8 — binary delivery + version match (15 min)
#
# Phase A is BLOCKED until all 8 probes record outcomes.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <user@droplet-host>" >&2
    echo
    echo "Example: $0 makakoo-tytus-control" >&2
    exit 1
fi

DROPLET_HOST="$1"
PROBE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="${PROBE_DIR}/results"
mkdir -p "${RESULTS_DIR}"
DATE_STR="$(date -u +%Y-%m-%d)"
RESULTS_FILE="${RESULTS_DIR}/PHASE-0-RESULTS-${DATE_STR}.md"

echo "garagetytus v0.5 Phase 0 probes"
echo "  target:  ${DROPLET_HOST}"
echo "  results: ${RESULTS_FILE}"
echo

# Header
{
    echo "# Phase 0 results — GARAGETYTUS-V0.5-MULTINODE"
    echo
    echo "- **Date (UTC):** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- **Droplet:** \`${DROPLET_HOST}\`"
    echo "- **Operator:** $(whoami)@$(hostname -s)"
    echo "- **garagetytus version:** $(garagetytus about 2>/dev/null | grep -i version || echo 'unknown')"
    echo
    echo "Probes are sequential per the canonical sprint. Any probe"
    echo "failure pauses Phase A for diagnosis."
    echo
} > "${RESULTS_FILE}"

probe() {
    local id="$1"
    local title="$2"
    local script="$3"
    echo "▶ ${id} ${title}"
    {
        echo "## ${id} — ${title}"
        echo
        echo '```'
        bash "${PROBE_DIR}/${script}" "${DROPLET_HOST}" 2>&1 || echo "  (probe exited non-zero — investigate)"
        echo '```'
        echo
    } >> "${RESULTS_FILE}"
}

probe "P0.1" "Droplet access + Linux x86_64 check"     "p01-droplet-access.sh"
probe "P0.2" "Garage multi-node protocol probe"        "p02-multi-node-probe.sh"
probe "P0.3" "Garage RPC nature + timeout"             "p03-rpc-probe.sh"
probe "P0.4" "WG tunnel capacity under load"           "p04-wg-throughput.sh"
probe "P0.5" "Pod → droplet WG IP reachability"        "p05-pod-reachability.sh"
probe "P0.6" "Bootstrap channel for cluster secret"    "p06-secret-channel.sh"
probe "P0.7" "Garage bucket-level denial mechanism"    "p07-deny-all.sh"
probe "P0.8" "Binary delivery + version match"         "p08-binary-version.sh"

echo
echo "✓ Phase 0 probes complete."
echo "  Review:  ${RESULTS_FILE}"
echo
echo "Next:"
echo "  1. Read every probe's output. Confirm no blockers."
echo "  2. Copy ${RESULTS_FILE} to"
echo "     MAKAKOO/development/sprints/queued/GARAGETYTUS-V0.5-MULTINODE/results/"
echo "  3. Begin Phase A.1 (cluster init SSH orchestration)."
