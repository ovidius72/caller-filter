# Host FFI boundary checks

Run from the repository root:

```sh
bash tools/test-ffi.sh
```

This builds the host library, generates **fresh Swift and Kotlin bindings** into
ignored `target/ffi-tests/`, creates synthetic test datasets, compiles both host
programs and executes them. It fails if either language cannot compile or run.
No generated bindings, compiled classes or binaries belong in git.

Prerequisites: Rust, Swift, Java and Gradle. The standalone JVM harness uses the
Kotlin compiler/runtime and JNA already in the local Gradle distribution. It
requires no Android SDK, NDK, plugin download or changes to the app project.
Tested on macOS arm64 with Swift 6.2.4, JDK 22 and Gradle 9.7.1 (embedded Kotlin
2.4.0, JNA 5.17.0). Generated Kotlin currently emits two harmless unused-expression
warnings. Other distributions may package different compiler dependencies;
missing dependencies must fail, never turn into a skipped Kotlin test.

## Covered through both generated bindings

- Runtime numbering/place payloads, content/schema versions and place prefixes.
- National/international normalization and missing runtime region errors.
- Place lookup, language fallback, NotGeographic, NoData and invalid input.
- Prepared rules, evaluation, allow exceptions, conflicts with both rule IDs,
  unsigned IDs above the signed integer range, and typed errors.
- Explanation verdicts/caveats and changed-content, same-version snapshot rejection.
- Actual foreign callback implementations: sorted bounded batches, aggregate
  cap before delivery, cancellation, declared errors, unexpected exceptions,
  reentrant calls and concurrent evaluation. Swift also checks callback release.

Fixtures in `numbering.xml` describe a **synthetic test-only** plan. Rust creates
CFNM/CFDS test bytes from it. `numbering-v1.hex` is a frozen, nonempty compatibility
fixture: do not regenerate it to hide a serialization change.

## Runtime contract

`Snapshot` opens owned bytes eagerly and publishes only after all inputs validate.
It is immutable and Send + Sync. A failed replacement cannot mutate an old handle.
Place files are supplied in preference order (user language, then fallback).
The API reports flat names and the selected language, not an invented hierarchy.

Rules resolved from places retain their exact snapshot and reject another snapshot
in explain/expand, even if content-version labels match. Direct rules are portable.
Version strings are diagnostics, not identity. Persist authored rules, not these
prepared handles or their derived expansions. P007 owns OTA delivery/replacement.

Evaluation uses prepared rules without loading datasets. Expansion is synchronous:
run it off the UI thread. No lock is held across foreign callbacks. The caller
chooses a nonzero batch size and must release batches to retain bounded memory.
`max_entries` limits the merged, deduplicated list after allow subtraction.
`max_candidates` limits **each deny's** candidate space, not the total rule set;
work scales with deny count and includes repeated iterator walks. If that bound
prevents counting, TooBroad carries an arithmetic ceiling and `exact=false`.
A completed merged count uses `exact=true`. Rule IDs identify the implicated denies.
No batch is delivered on TooBroad or NotExpandable. Cancellation happens after a
batch, not during preflight. An exception stops delivery but cannot undo batches
already accepted; shells must stage output rather than publish partial artifacts.

CFNM v1 stores the phonenumber 0.3.10 loader DTO with postcard. The core pins that
crate version; a DTO/layout change requires a format-version decision. Runtime
content versions can be newer than the engine. Invalid descriptors are rejected
before the dependency's panic-prone constructors. Its compiled country enum still
limits **brand-new region IDs**: unknown shared-code regions are refused, while
an unknown single-code geographic region cannot normalize successfully. Existing
region metadata updates work at runtime. This inherited dependency constraint is
not solved by this FFI slice; no country allowlist was added.

## Memory and startup evidence

```sh
cargo run --release -p callerfilter-datasets --bin build-number-metadata
cargo test --release -p callerfilter-core --test ffi_memory \
  runtime_metadata_cold_and_coexisting_databases -- --ignored --exact --nocapture
cargo test --release -p callerfilter-core --test ffi_memory \
  streaming_ffi_peak_does_not_scale_with_output -- --exact --nocapture
```

Measured on this macOS arm64 host, with vendored metadata 9.0.33:

- CFNM file: 526,596 bytes.
- Isolated cold runtime parse: 112.18 ms, 4,153,627-byte peak Rust allocation,
  1,863,810 bytes retained. No XML conversion or bundled database initialization
  runs in the measurement process.
- New database while the old remains live: 96.07 ms; combined peak 6,017,437 bytes.
- FFI streaming 1,000 versus 10,000 numbers, batch size 32: 887 versus 888 bytes
  peak above the warmed baseline. Collecting the same 10,000 numbers: 197,576 bytes.

These are measurements, not limits. They exclude allocator overhead, RSS, foreign
heaps, place datasets and OS extension overhead; device budgets remain unverified.
Release test builds currently warn about Cargo duplicate lib-output paths when
building the workspace's multi-crate-type library with test/abort variants; tests
execute successfully. No profile or existing core algorithm was changed to hide it.
