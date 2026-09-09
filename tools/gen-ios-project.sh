#!/bin/bash
# Generate the Xcode project, taking the signing team from apps/ios/.env so no
# personal team ID is ever committed.
set -euo pipefail
cd "$(dirname "$0")/../apps/ios"

if [ ! -f .env ]; then
  echo "apps/ios/.env is missing. Copy .env.example to .env and set DEVELOPMENT_TEAM." >&2
  exit 1
fi
set -a; . ./.env; set +a

if [ -z "${DEVELOPMENT_TEAM:-}" ]; then
  echo "DEVELOPMENT_TEAM is empty in apps/ios/.env." >&2
  exit 1
fi

xcodegen generate
echo "Generated CallerFilter.xcodeproj for team $DEVELOPMENT_TEAM"
