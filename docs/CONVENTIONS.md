# Conventions

Agreed once, so they are not argued per pull request. The rules about *what the
code may contain* are in the planner's Project Guidelines; this is about how the
work is organised.

## Formatting and linting

| | tool | enforced |
|---|---|---|
| Rust | `cargo fmt`, `cargo clippy -D warnings` | CI, blocking |
| Guidelines §1 | `tools/check-no-hardcoded-numbering.sh` | CI, blocking |
| Swift | Xcode defaults | not yet enforced |
| Kotlin | AGP defaults | not yet enforced |

Swift and Kotlin linting is deliberately deferred: both apps are currently thin
shells with almost no code, so a linter would be enforcing style on nothing. Add
one when either app has real UI — which is F004 and F005.

## Where tests go, and where the weight belongs

**The core carries the heaviest coverage in this project.** It is pure logic with
no platform dependency, so it tests on the host with no device or simulator, and
a bug in it is a bug on every platform simultaneously. The rule model and the
range expander deserve the most tests of anything here.

- Core unit tests: `#[cfg(test)] mod tests` beside the code.
- Anything needing a device is not a test. It is a harness, run by hand, and
  lives under `tools/` — see `tools/entry-limit-harness/`.

The apps get tests when they have logic worth testing. Today they call one core
function to prove linkage, and asserting that would test the FFI, not the app.

## Dependencies

- Workspace-level versions in the root `Cargo.toml`, so the core and any future
  crate cannot drift apart.
- Prefer few dependencies. The core links into iOS extensions that run under
  tight memory limits.
- **libphonenumber data is an input, not a dependency.** It is fetched and
  converted by the build (F003), never vendored into the tree. Updating it is a
  data change, not a code change — that is the whole point of Guidelines §1.

## What is never committed

- Generated UniFFI bindings. They drift from the core in silence and fail at
  runtime on one platform only.
- Packaged datasets. A committed one defeats over-the-air versioning; a silently
  truncated one makes rules stop matching with no error anywhere.
- The Xcode project. It is generated from `apps/ios/project.yml` — edit the yml.
- `apps/ios/.env`. Team IDs are personal; a committed one breaks every other
  developer. Copy `.env.example`.

## Builds

```sh
cargo test                          # host, fast, no device
./tools/check-no-hardcoded-numbering.sh
./tools/build-ios.sh                # 3 slices, bindings, XCFramework, sizes
./tools/build-android.sh            # 3 ABIs, bindings, sizes
./tools/gen-ios-project.sh          # reads apps/ios/.env
```

CI runs all of it plus both app builds. It does **not** run the entry-limit
harness — that needs a real iPhone and someone watching.

## Judging binary size

From **linked binaries only**. The static `.a` is ~49 MB and means nothing — the
same figure appears with every dependency removed. Current real sizes: iOS app
528K, each extension 492K, whole `.app` 1.6M; Android core 232K–364K per ABI. CI
reports both on every run.

## Toolchain notes that will bite you

- **Build iOS Release.** Xcode 26 Debug builds put target code in a separate
  `__preview.dylib`, so an extension's principal class is missing from the
  `.appex` and it fails to enable with a generic error.
- **Kotlin bindings come from the host library**, not the Android `.so`. Stripping
  removes UniFFI's metadata from ELF; Mach-O keeps it.
- **The dev machine runs JDK 26**, which forces AGP 9 on Gradle 9 and means the
  separate `kotlin.android` plugin must stay omitted. CI pins JDK 21 instead, so
  it does not depend on what any developer happens to have installed.
- **Generated Kotlin goes on the `kotlin` source set**, not `java`.
