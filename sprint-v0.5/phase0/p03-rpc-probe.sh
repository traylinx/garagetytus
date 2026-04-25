#!/usr/bin/env bash
# P0.3 — Garage RPC protocol nature + timeout tolerance (30 min)
#
# Determines whether Garage's RPC port (3901) speaks HTTP-wrappable
# bytes or custom binary TCP, and whether default RPC timeouts
# survive the WG tunnel's RTT (~24ms) + bandwidth (~4 KB/s baseline).
# Outcome shapes Phase A (firewall rules) + v0.9 edge-HTTPS theme.
set -euo pipefail
HOST="$1"

echo "▸ Garage version on droplet (if installed):"
ssh "${HOST}" "command -v garage && garage --version" 2>&1 || echo "  garage not yet installed — install in Phase A.1"
echo
echo "▸ Probe RPC port nature (does it speak HTTP?):"
ssh "${HOST}" "
    if command -v garage >/dev/null 2>&1 && pgrep -f 'garage server' >/dev/null; then
        curl -s --max-time 2 -o /dev/null -w 'HTTP code: %{http_code}\n' http://127.0.0.1:3901/ || echo '  RPC port did not respond to HTTP probe (likely custom binary protocol)'
    else
        echo '  garage server not running — re-probe after Phase A.1'
    fi
"
echo
echo "▸ Garage RPC timeout defaults (search source for rpc_timeout):"
echo "  Document the value found in upstream Garage docs at:"
echo "    https://garagehq.deuxfleurs.fr/documentation/reference-manual/configuration/"
echo "  Look for: rpc_timeout, heartbeat_interval, ping_timeout"
echo
echo "Acceptance gates:"
echo "  [ ] RPC protocol nature recorded (HTTP-wrappable Y/N)"
echo "  [ ] Default rpc_timeout value recorded"
echo "  [ ] Verdict: does 24ms WG RTT cause heartbeat failures? (Y/N)"
