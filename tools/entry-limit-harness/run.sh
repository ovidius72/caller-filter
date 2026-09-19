#!/bin/bash
# Build only by default. Installation is a separate, explicitly targeted command.
# Legacy: ./run.sh N [blocking|identification] [--sign --device DEVICE_ID]
# Probe:  ./run.sh build --mode empty --alias control
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
if [ -f "$ROOT/apps/ios/.env" ]; then
  set -a
  . "$ROOT/apps/ios/.env"
  set +a
fi
if [[ ${1:-} =~ ^[0-9]+$ ]]; then
  COUNT=$1
  shift
  MODE=blocking
  if [[ ${1:-} == blocking || ${1:-} == identification ]]; then
    MODE=$1
    shift
  fi
  exec python3 "$ROOT/tools/device-tests/ios-probe.py" build \
    --mode "$MODE" --count "$COUNT" --alias capacity "$@"
fi
exec python3 "$ROOT/tools/device-tests/ios-probe.py" "$@"
