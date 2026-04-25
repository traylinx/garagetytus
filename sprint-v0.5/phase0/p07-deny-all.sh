#!/usr/bin/env bash
# P0.7 — Garage bucket-level denial mechanism (20 min)
#
# pi-flagged: the round-0 draft assumed an admin-API "deny-all"
# endpoint that may not exist. This probe finds the REAL call shape
# Garage exposes for bucket-level denial. LD#7 + Phase C.3
# implementation depend on the outcome.
set -euo pipefail
HOST="$1"

echo "▸ Probe: garage CLI bucket-level deny:"
ssh "${HOST}" "
    if command -v garage >/dev/null 2>&1; then
        garage bucket --help 2>&1 | head -30
        echo '---'
        garage bucket allow --help 2>&1 | head -10
        echo '---'
    else
        echo '  garage not installed yet — re-probe after Phase A.1'
    fi
"
echo
echo "▸ Probe: S3 PutBucketPolicy with deny-all policy:"
echo "  After Phase A.1 lands, test:"
echo
echo "    aws s3api put-bucket-policy --bucket test-deny \\"
echo "        --policy '{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Deny\",\"Principal\":\"*\",\"Action\":\"s3:*\",\"Resource\":\"arn:aws:s3:::test-deny/*\"}]}' \\"
echo "        --endpoint-url http://10.42.42.1:3900"
echo
echo "  Then verify:"
echo "    aws s3 cp foo.txt s3://test-deny/ --endpoint-url ... → AccessDenied?"
echo "    aws s3api get-object ... with valid presigned URL → AccessDenied?"
echo
echo "Acceptance gates (locks LD#7 + Phase C.3 implementation):"
echo "  [ ] Verified call shape for bucket-level deny"
echo "  [ ] Does it deny presigned URLs already issued? (Y/N)"
echo "  [ ] Propagation timing across cluster nodes (seconds)"
echo "  [ ] If no usable deny: document fallback to per-key revoke + TTL"
