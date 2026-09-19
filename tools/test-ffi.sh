#!/bin/bash
# Fresh host bindings, real callbacks in both languages. No generated sources committed.
set -euo pipefail
cd "$(dirname "$0")/.."

# Deliberately separate from the Rust suite; final verification runs each once.
cargo build --profile bindings -p callerfilter-core
library=$(bash tools/host-cdylib.sh callerfilter_core bindings)
mkdir -p target/ffi-tests/swift target/ffi-tests/kotlin
cargo test -p callerfilter-core --test ffi write_host_fixtures -- --ignored
for language in swift kotlin; do
  cargo run --profile bindings --bin uniffi-bindgen -- generate \
    --library "$library" --language "$language" \
    --out-dir "target/ffi-tests/$language" --no-format
done
swiftc target/ffi-tests/swift/callerfilter_core.swift tools/ffi-tests/SwiftHostSmoke.swift \
  -import-objc-header target/ffi-tests/swift/callerfilter_coreFFI.h \
  -L target/bindings -lcallerfilter_core -o target/ffi-tests/swift-smoke
DYLD_LIBRARY_PATH="$PWD/target/bindings${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
LD_LIBRARY_PATH="$PWD/target/bindings${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  target/ffi-tests/swift-smoke
gradle -p tools/ffi-tests --offline --console=plain \
  --project-cache-dir "$PWD/target/ffi-tests/gradle-cache" hostTest
