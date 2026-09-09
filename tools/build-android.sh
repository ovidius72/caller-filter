#!/bin/bash
# Build the Rust core for Android and generate the Kotlin bindings.
#
# Bindings and the .so files are BUILD OUTPUTS under apps/android/app/generated/.
# Never commit them: a checked-in binding drifts from the core in silence and
# fails at runtime on one platform only.
set -euo pipefail
cd "$(dirname "$0")/.."

LIB=callerfilter_core
GEN=apps/android/app/generated
PROFILE=release

: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
export ANDROID_HOME
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)}"

if [ ! -d "${ANDROID_NDK_HOME:-/nonexistent}" ]; then
  echo "No Android NDK found under $ANDROID_HOME/ndk." >&2
  echo "Install one:  sdkmanager --sdk_root=\"\$ANDROID_HOME\" 'ndk;29.0.14206865'" >&2
  exit 1
fi
echo "==> NDK: $ANDROID_NDK_HOME"

rm -rf "$GEN"
mkdir -p "$GEN/kotlin" "$GEN/jniLibs"

echo "==> building core for Android ABIs"
cargo ndk -o "$GEN/jniLibs" \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  build --profile $PROFILE -p callerfilter-core

# The release profile strips, and stripping removes the metadata UniFFI reads
# from an ELF .so. Mach-O keeps it, which is why the iOS build can read its own
# artifact. Bindings describe the crate's interface, not the target, so generate
# them from the host library instead.
echo "==> generating Kotlin bindings (from the host library)"
cargo build --profile $PROFILE -p callerfilter-core
cargo run --profile $PROFILE --bin uniffi-bindgen -- generate \
  --library "$(tools/host-cdylib.sh "$LIB" "$PROFILE")" \
  --language kotlin --out-dir "$GEN/kotlin" --no-format

echo
echo "==> core size per ABI (watch this)"
find "$GEN/jniLibs" -name "*.so" | while read -r f; do
  printf "    %-46s %s\n" "${f#"$GEN/jniLibs/"}" "$(du -h "$f" | cut -f1)"
done
echo
echo "Kotlin bindings: $GEN/kotlin"
