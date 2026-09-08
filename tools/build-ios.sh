#!/bin/bash
# Build the Rust core for iOS and package it as an XCFramework, then generate
# the Swift bindings.
#
# Bindings and the XCFramework are BUILD OUTPUTS. Never commit them: a checked-in
# binding drifts from the core in silence and fails at runtime on one platform.
set -euo pipefail
cd "$(dirname "$0")/.."

LIB=callerfilter_core
OUT=build/ios
PROFILE=release

echo "==> building core for iOS targets"
cargo build --profile $PROFILE --target aarch64-apple-ios      -p callerfilter-core
cargo build --profile $PROFILE --target aarch64-apple-ios-sim  -p callerfilter-core
cargo build --profile $PROFILE --target x86_64-apple-ios       -p callerfilter-core

rm -rf "$OUT"
mkdir -p "$OUT/sim" "$OUT/headers"

# One simulator slice covering both Apple Silicon and Intel Macs.
lipo -create \
  "target/aarch64-apple-ios-sim/$PROFILE/lib$LIB.a" \
  "target/x86_64-apple-ios/$PROFILE/lib$LIB.a" \
  -output "$OUT/sim/lib$LIB.a"

echo "==> generating Swift bindings"
rm -rf bindings/swift
cargo run --profile $PROFILE --bin uniffi-bindgen -- generate \
  --library "target/aarch64-apple-ios/$PROFILE/lib$LIB.dylib" \
  --language swift --out-dir bindings/swift

# The XCFramework needs the header and a modulemap named module.modulemap.
cp bindings/swift/${LIB}FFI.h "$OUT/headers/"
cp bindings/swift/${LIB}FFI.modulemap "$OUT/headers/module.modulemap"

echo "==> packaging XCFramework"
rm -rf "$OUT/$LIB.xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$PROFILE/lib$LIB.a" -headers "$OUT/headers" \
  -library "$OUT/sim/lib$LIB.a"                          -headers "$OUT/headers" \
  -output "$OUT/$LIB.xcframework" > /dev/null

echo
echo "==> core size per slice (watch this: the iOS extensions run under tight memory limits)"
for a in "target/aarch64-apple-ios/$PROFILE/lib$LIB.a" "$OUT/sim/lib$LIB.a"; do
  printf "    %-58s %s\n" "$a" "$(du -h "$a" | cut -f1)"
done
echo
echo "XCFramework: $OUT/$LIB.xcframework"
echo "Swift bindings: bindings/swift/$LIB.swift"
