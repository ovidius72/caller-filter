#!/bin/bash
# Print the path to a crate's shared library built for the HOST.
#
# UniFFI reads a crate's interface metadata out of a built library. The Android
# build cannot use its own .so for this — the release profile strips, and
# stripping removes that metadata from an ELF object — so it reads the host
# library instead. The host library's extension is .dylib on macOS and .so on
# Linux, and CI is Linux while developer machines here are macOS.
#
# Ask for the file rather than guessing the extension from the platform, so no
# caller has to know any of the above.
#
# Usage: host-cdylib.sh <lib_name> <profile>
#   e.g. host-cdylib.sh callerfilter_core release  ->  target/release/libcallerfilter_core.so
set -euo pipefail

lib=$1
profile=$2
dir="target/$profile"

for path in "$dir/lib$lib".dylib "$dir/lib$lib".so; do
  if [ -f "$path" ]; then
    echo "$path"
    exit 0
  fi
done

echo "No host library for '$lib' under $dir/ (looked for lib$lib.dylib and lib$lib.so)." >&2
echo "Build it first:  cargo build --profile $profile" >&2
exit 1
