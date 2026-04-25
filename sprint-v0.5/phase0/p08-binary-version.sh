#!/usr/bin/env bash
# P0.8 — Binary delivery + version match (15 min)
#
# Confirms the Garage binary on Mac and droplet will be the same
# version. LD#16 of canonical sprint: cluster refuses mismatched
# Garage versions across nodes; upgrades go droplet-first.
set -euo pipefail
HOST="$1"

echo "▸ Mac-side Garage version:"
local_ver="$(garage --version 2>&1 | head -1 || echo 'not installed')"
echo "  ${local_ver}"
echo
echo "▸ Droplet Garage version (if installed):"
ssh "${HOST}" "command -v garage && garage --version 2>&1 | head -1 || echo 'not yet installed'"
echo
echo "▸ Pinned upstream URL + SHA from versions.toml:"
if [[ -f versions.toml ]]; then
    grep -E 'version|target|url|sha256' versions.toml | head -20
elif [[ -f ../../versions.toml ]]; then
    grep -E 'version|target|url|sha256' ../../versions.toml | head -20
else
    echo "  versions.toml not found in PWD or repo root"
fi
echo
echo "▸ End-to-end SHA-256 verification probe:"
echo "  ssh ${HOST} \"curl -sL <pinned-url> | sha256sum\""
echo "  Compare against versions.toml's pinned hash."
echo
echo "Acceptance gates (locks LD#16 + Phase A.1 install):"
echo "  [ ] Mac + droplet will run the same Garage version"
echo "  [ ] SHA-256 of downloaded binary matches versions.toml pin"
echo "  [ ] Phase A.1 install method: SSH-triggered curl + sha256 verify"
