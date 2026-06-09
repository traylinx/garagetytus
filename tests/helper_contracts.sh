#!/usr/bin/env bash
# Regression contracts for Tytus shared-folder helpers. These scripts run on
# macOS launchd with /bin/bash 3.2, so keep them Bash-3-compatible.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE_FILE="$(mktemp)"
trap 'rm -f "$STATE_FILE"' EXIT

cat > "$STATE_FILE" <<'JSON'
{
  "secret_key": "",
  "agent_user_id": "",
  "pods": [
    {
      "pod_id": "01",
      "route_id": "0e0ah755r3",
      "id": "0e0ah755r3",
      "droplet_ip": "203.0.113.10",
      "agent_type": "nemoclaw"
    }
  ],
  "agents": [],
  "included": []
}
JSON

for helper in \
  garagetytus-refresh-watchdog \
  garagetytus-pod-provision \
  garagetytus-pod-refresh \
  garagetytus-pod-deprovision \
  garagetytus-folder-status; do
  /bin/bash -n "$ROOT/bin/$helper"
done

if grep -R --line-number --fixed-strings 'mapfile' "$ROOT/bin/garagetytus-refresh-watchdog" "$ROOT/bin/garagetytus-pod-provision" "$ROOT/bin/garagetytus-pod-refresh" "$ROOT/bin/garagetytus-pod-deprovision" "$ROOT/bin/garagetytus-folder-status"; then
  echo "Bash 4-only mapfile is forbidden in Tytus helpers; macOS /bin/bash is 3.2" >&2
  exit 1
fi

# Initial folder bind resync must have a much longer timeout than normal poll
# runs. A short timeout can kill `rclone bisync --resync` mid-baseline and poison
# all future folder syncs.
grep -F 'TIMEOUT_SEC=300' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'RESYNC_TIMEOUT_SEC=1800' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '/usr/local/bin/timeout "$RESYNC_TIMEOUT_SEC" rclone' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '/usr/local/bin/timeout ${TIMEOUT_SEC} /usr/local/bin/rclone' "$ROOT/bin/garagetytus-folder-bind" >/dev/null

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-provision" 0e0ah755r3 --bucket missions --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01 on root@203.0.113.10'

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-refresh" 0e0ah755r3 --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01 on root@203.0.113.10'

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-deprovision" 0e0ah755r3 --bucket missions --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01 on root@203.0.113.10'
