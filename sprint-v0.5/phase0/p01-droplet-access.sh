#!/usr/bin/env bash
# P0.1 — Droplet access + Linux x86_64 check (15 min)
#
# Verifies SSH works, droplet is x86_64 Linux with sufficient disk
# + outbound HTTPS for the Garage tarball download. Aborts the
# sprint if any precondition fails.
set -euo pipefail
HOST="$1"

echo "▸ uname:"
ssh -o ConnectTimeout=10 "${HOST}" "uname -a"
echo
echo "▸ disk free (root):"
ssh "${HOST}" "df -h / | tail -1"
echo
echo "▸ memory:"
ssh "${HOST}" "free -h | head -2"
echo
echo "▸ outbound HTTPS to GitHub:"
ssh "${HOST}" "curl -sI https://github.com | head -1"
echo
echo "▸ sudo without prompt:"
ssh "${HOST}" "sudo -n whoami" 2>&1 || echo "  (no NOPASSWD sudo — installer will require interactive sudo)"
echo
echo "▸ systemd-user availability:"
ssh "${HOST}" "command -v systemctl && systemctl --user status 2>&1 | head -3 || echo 'systemd --user not configured'"
echo
echo "Acceptance gates (LD#9 of canonical sprint):"
echo "  [ ] x86_64 in uname output"
echo "  [ ] disk free ≥ 20 GB"
echo "  [ ] outbound HTTPS works"
