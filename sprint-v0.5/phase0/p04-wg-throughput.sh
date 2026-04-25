#!/usr/bin/env bash
# P0.4 — WG tunnel capacity UNDER LOAD (30 min)
#
# Measures WG throughput baseline + under-load behavior. Critical
# input: Garage replication runs over this tunnel; if throughput
# drops to 400 B/s under load, large-object replication blows up
# from minutes to hours.
set -euo pipefail
HOST="$1"

echo "▸ Baseline WG throughput (iperf3 if available):"
if command -v iperf3 >/dev/null 2>&1; then
    ssh "${HOST}" "command -v iperf3 || sudo apt-get install -y iperf3 2>&1 | tail -3"
    echo "  Run on droplet first: ssh ${HOST} 'iperf3 -s -1' &"
    echo "  Then locally: iperf3 -c <droplet-wg-ip> -t 10"
    echo "  (manual sequence — do not auto-run, it's interactive)"
else
    echo "  iperf3 not on PATH — install via brew (Mac) or apt (Linux)"
fi
echo
echo "▸ Large-object probe (10 MB write through WG):"
echo "  dd if=/dev/urandom of=/tmp/10mb bs=1M count=10"
echo "  time scp /tmp/10mb ${HOST}:/tmp/10mb"
echo "  Compare against the LD#3 baseline assumption (~4 KB/s)"
echo
echo "▸ Simulated load probe (per pi):"
echo "  Run a parallel rsync hammering the tunnel while a small"
echo "  test write goes through. Measure the test write's latency."
echo "  If load drops throughput to 400 B/s → landmine for"
echo "  large-object replication."
echo
echo "Acceptance gates:"
echo "  [ ] Baseline throughput recorded (KB/s or MB/s)"
echo "  [ ] Under-load throughput recorded"
echo "  [ ] 10 MB transfer time recorded"
echo "  [ ] Decision: edge-HTTPS-for-RPC needed in v0.9? (Y/N)"
