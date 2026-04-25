#!/usr/bin/env bash
# P0.6 — Bootstrap channel for cluster RPC secret (15 min)
#
# Verifies the SSH-based secret transport works + lands at the
# right path with the right perms. LD#5 of the canonical sprint:
# secret travels over SSH (encrypted), NEVER the WG tunnel.
set -euo pipefail
HOST="$1"

echo "▸ SSH key auth working (no password prompt):"
ssh -o BatchMode=yes "${HOST}" "true" 2>&1 && echo "  ✓ key auth ok" || echo "  ✗ password required — fix SSH config first"
echo
echo "▸ Test write to /etc/garagetytus/cluster_rpc_secret (mode 0600 root):"
TEST_VALUE="probe-$(date +%s)"
echo "  Test value: ${TEST_VALUE}"
echo
ssh "${HOST}" "
    set -e
    sudo mkdir -p /etc/garagetytus
    echo '${TEST_VALUE}' | sudo tee /etc/garagetytus/cluster_rpc_secret.probe >/dev/null
    sudo chmod 0600 /etc/garagetytus/cluster_rpc_secret.probe
    sudo chown root:root /etc/garagetytus/cluster_rpc_secret.probe
    ls -la /etc/garagetytus/cluster_rpc_secret.probe
    sudo cat /etc/garagetytus/cluster_rpc_secret.probe
    sudo rm /etc/garagetytus/cluster_rpc_secret.probe
"
echo
echo "▸ Confirm SSH transport (not WG):"
echo "  SSH always uses TCP/22 (or whatever port). NOT the WG tunnel."
echo "  Verify by checking ssh -v output: connection should hit the"
echo "  droplet's public IP, not 10.42.42.1."
echo
echo "Acceptance gates:"
echo "  [ ] SSH key auth works without password"
echo "  [ ] Probe file written + read with mode 0600 owner root"
echo "  [ ] SSH transport uses droplet's public IP (not WG)"
