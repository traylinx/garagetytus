#!/usr/bin/env bash
# P0.5 — Pod → droplet WG IP reachability (15 min)
#
# CRITICAL probe — both pi + qwen caught the original
# `localhost:3900` error in the round-0 draft. Pod netns is
# SEPARATE from droplet host netns. This probe confirms the WG-IP
# path works.
#
# If TCP path works → Phase A.2 binds Garage on 0.0.0.0:3900 with
# iptables restricted to 10.42.42.0/24.
# If TCP path fails → diagnose Tytus pod firewall; Phase C
# fallback (MCP shim only) becomes the v0.5 ship.
set -euo pipefail
HOST="$1"

echo "▸ From a running tytus pod, probe the droplet's WG IP:"
echo
echo "  Run this manually (needs an active pod):"
echo "    tytus exec --pod 02 \"curl -v http://10.42.42.1:3900/ 2>&1 | head -20\""
echo
echo "  Expected during this probe (Garage NOT yet bound on droplet):"
echo "    - TCP SYN reaches the droplet (curl gets connection refused, NOT timeout)"
echo "    - tcpdump on droplet shows incoming SYN from pod's WG address"
echo
echo "  After Phase A.2 (Garage bound on 10.42.42.1:3900):"
echo "    - HTTP 200 response with Garage's S3 API banner"
echo
echo "▸ If you have an active pod, capture tcpdump on the droplet:"
echo "    ssh ${HOST} \"sudo tcpdump -i wg0 -n -c 5 'tcp port 3900'\" &"
echo "    tytus exec --pod 02 \"curl --max-time 3 http://10.42.42.1:3900/\""
echo
echo "Acceptance gates (locks Phase A.2 binding decision):"
echo "  [ ] Pod TCP SYN reaches droplet on port 3900? (Y/N)"
echo "  [ ] If N: Tytus pod firewall blocking? Document for Phase C fallback"
echo "  [ ] If Y: confirms 0.0.0.0:3900 + iptables 10.42.42.0/24 is safe"
