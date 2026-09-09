#!/bin/bash
# usage: ./run.sh N mode
set -e
N=$1; MODE=${2:-blocking}
cd "$(dirname "$0")"
# Team id comes from apps/ios/.env — never hardcode a personal one.
[ -f ../../apps/ios/.env ] && { set -a; . ../../apps/ios/.env; set +a; }
: "${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM in apps/ios/.env}"
sed -i '' "s/LimitTestN: \"[0-9]*\"/LimitTestN: \"$N\"/" project.yml
sed -i '' "s/LimitTestMode: .*/LimitTestMode: $MODE/" project.yml
xcodegen generate > /dev/null 2>&1
xcodebuild -project LimitTest.xcodeproj -scheme LimitTest \
  -destination 'id=00008110-001208843C32801E' -allowProvisioningUpdates \
  -configuration Release -derivedDataPath ./dd ENABLE_DEBUG_DYLIB=NO build > /tmp/lt_build.log 2>&1 || { echo "BUILD FAILED"; grep -a "error:" /tmp/lt_build.log | head -3; exit 1; }
xcrun devicectl device install app --device 00008110-001208843C32801E \
  ./dd/Build/Products/Release-iphoneos/LimitTest.app > /tmp/lt_install.log 2>&1 || { echo "INSTALL FAILED"; exit 1; }
echo "installed with N=$N mode=$MODE"
