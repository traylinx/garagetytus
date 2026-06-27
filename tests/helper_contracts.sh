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
  garagetytus-folder-bind \
  garagetytus-folder-sync \
  garagetytus-refresh-watchdog \
  garagetytus-folder-materialize \
  garagetytus-pod-provision \
  garagetytus-pod-refresh \
  garagetytus-pod-deprovision \
  garagetytus-folder-status; do
  /bin/bash -n "$ROOT/bin/$helper"
done

if grep -R --line-number --fixed-strings 'mapfile' "$ROOT/bin/garagetytus-folder-sync" "$ROOT/bin/garagetytus-refresh-watchdog" "$ROOT/bin/garagetytus-folder-materialize" "$ROOT/bin/garagetytus-pod-provision" "$ROOT/bin/garagetytus-pod-refresh" "$ROOT/bin/garagetytus-pod-deprovision" "$ROOT/bin/garagetytus-folder-status"; then
  echo "Bash 4-only mapfile is forbidden in Tytus helpers; macOS /bin/bash is 3.2" >&2
  exit 1
fi

# Initial folder bind resync must have a much longer timeout than normal poll
# runs. A short timeout can kill `rclone bisync --resync` mid-baseline and poison
# all future folder syncs.
grep -F 'TIMEOUT_SEC=300' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'RESYNC_TIMEOUT_SEC=7200' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '/usr/local/bin/garagetytus-folder-sync' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '<string>--resync-timeout</string>' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '<string>--remote-path</string>' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '<string>--workdir</string>' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'unload_existing_auto_sync' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'sync_status:{state:$sync_state' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'remote:$remote, remote_path:$remote_path' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'write_binding_sidecar "provisioning" "${PODS[@]}"' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'write_binding_sidecar "materialized" "${provisioned_pods[@]}"' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'initial bisync --resync' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'has_baseline' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F -- '--conflict-resolve newer' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'DEFAULT_EXCLUDES=(' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F '"**/node_modules/**"' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F '"**/venv/**"' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
# Mac↔pod exclusion symmetry: the build/dep/IaC floor must match the pod-side
# contract shared-folder-upload-exclude-v1 (target/dist/build/vendor/.terraform/
# site-packages). A Rust/Terraform project bound as a shared folder otherwise
# pushes 10k+ generated artifacts and the first sync looks frozen.
grep -F '"**/target/**"' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F '"**/.terraform/**"' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F '"**/target/**"' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F '"**/.terraform/**"' "$ROOT/bin/garagetytus-folder-bind" >/dev/null
grep -F 'GARAGETYTUS_SYNC_ALL' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'RCLONE_FILTER_ARGS+=(--exclude "$pattern")' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'clear_stale_locks' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'active rclone lock kept' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'active rclone lock present; skipping this tick' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'acquire_global_sync_slot' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'GARAGETYTUS_SYNC_PARALLEL' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'another garagetytus sync is already running' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'skip_recent_automatic_incremental' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'COOLDOWN_SEC=600' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F -- '--force) FORCE=1' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F '.last-attempt' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F 'MATERIALIZE_MODE="${GARAGETYTUS_MATERIALIZE_MODE:-background}"' "$ROOT/bin/garagetytus-pod-provision" >/dev/null
grep -F 'materialize_args+=(--background)' "$ROOT/bin/garagetytus-pod-provision" >/dev/null
grep -F -- '--background) BACKGROUND=1' "$ROOT/bin/garagetytus-folder-materialize" >/dev/null
grep -F 'queued background materialization' "$ROOT/bin/garagetytus-folder-materialize" >/dev/null
if grep -F 'exec /usr/local/bin/timeout' "$ROOT/bin/garagetytus-folder-sync" >/dev/null; then
  echo "folder-sync must not exec timeout; the shell owns the global sync lock/trap" >&2
  exit 1
fi
if grep -F -- '--create-empty-src-dirs' "$ROOT/bin/garagetytus-folder-sync" >/dev/null; then
  echo "folder-sync must not sync empty dirs to S3; it slows/hangs bisync finalization" >&2
  exit 1
fi
grep -F -- '--stats 15s' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
grep -F -- '--stats-one-line' "$ROOT/bin/garagetytus-folder-sync" >/dev/null
python3 - "$ROOT/bin/garagetytus-folder-bind" <<'PY'
import sys
body = open(sys.argv[1], encoding="utf-8").read()
must_order = [
    '\nunload_existing_auto_sync\n',
    'write_binding_sidecar "provisioning" "${PODS[@]}"',
    '    install_auto_sync_plist\n',
    'phase 6/7 — provision pods',
    'write_binding_sidecar "materialized" "${provisioned_pods[@]}"',
]
pos = [body.index(token) for token in must_order]
if pos != sorted(pos):
    raise SystemExit("folder-bind ordering regression: bind registration/background sync lifecycle is unsafe")
PY

python3 - "$ROOT/bin/garagetytus-folder-bind" <<'PY'
import sys
body = open(sys.argv[1], encoding="utf-8").read()
forbidden = [
    'else "wannolot-" + . end',
    'provisioned_pods+=("wannolot-${pod}")',
    'provisioned_pods+=("wannolot-${pod##*-}")',
]
for token in forbidden:
    if token in body:
        raise SystemExit(f"folder-bind route selector regression: found {token!r}")
required = [
    'def selector: if test("^(wannolot|tytus)-") then sub("^(wannolot|tytus)-"; "") else . end;',
    'if ($sel | test("^[0-9]+$")) then "wannolot-" + $sel else $sel end',
    'provisioned_pods+=("$pod")',
    'provision_selector: $sel',
]
for token in required:
    if token not in body:
        raise SystemExit(f"folder-bind route selector contract missing {token!r}")
PY

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-provision" 0e0ah755r3 --bucket missions --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01; pod I/O via tytus route, Garage via root@203.0.113.10'

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-refresh" 0e0ah755r3 --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01 on root@203.0.113.10'

TYTUS_STATE_PATH="$STATE_FILE" /bin/bash "$ROOT/bin/garagetytus-pod-deprovision" 0e0ah755r3 --bucket missions --dry-run 2>&1 \
  | grep -F 'resolved route selector 0e0ah755r3 -> pod 01 on root@203.0.113.10'
