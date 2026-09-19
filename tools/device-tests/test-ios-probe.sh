#!/bin/bash
# Host-only checks. No simulator, device installation, extension reload or calls.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$ROOT/target/device-tests/ios-host-tests"
mkdir -p "$OUT"
swiftc -warnings-as-errors \
  "$ROOT/tools/entry-limit-harness/Shared/HarnessConfiguration.swift" \
  "$ROOT/tools/entry-limit-harness/Shared/HarnessLoadState.swift" \
  "$ROOT/tools/device-tests/ios-probe-tests.swift" -o "$OUT/probe-tests"
"$OUT/probe-tests" "$ROOT/tools/entry-limit-harness/harness-defaults.json"
PYTHONDONTWRITEBYTECODE=1 python3 "$ROOT/tools/device-tests/test-ios-probe.py"
bash -n "$ROOT/tools/entry-limit-harness/run.sh" "$ROOT/tools/device-tests/test-ios-probe.sh"
