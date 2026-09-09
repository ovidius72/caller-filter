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
BINDINGS_PROFILE=bindings

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

# Bindings are read out of a built library's UniFFI metadata. The shipped .so
# cannot supply it: the release profile strips, and stripping removes that
# metadata from an ELF object. Bindings describe the crate's interface and not
# the target, so build for the host under the unstripped `bindings` profile and
# read that. See [profile.bindings] in the workspace Cargo.toml.
echo "==> generating Kotlin bindings (from the host library)"
cargo build --profile $BINDINGS_PROFILE -p callerfilter-core
cargo run --profile $PROFILE --bin uniffi-bindgen -- generate \
  --library "$(tools/host-cdylib.sh "$LIB" "$BINDINGS_PROFILE")" \
  --language kotlin --out-dir "$GEN/kotlin" --no-format

echo
echo "==> core size per ABI (watch this)"
find "$GEN/jniLibs" -name "*.so" | while read -r f; do
  printf "    %-46s %s\n" "${f#"$GEN/jniLibs/"}" "$(du -h "$f" | cut -f1)"
done
echo
echo "Kotlin bindings: $GEN/kotlin"
