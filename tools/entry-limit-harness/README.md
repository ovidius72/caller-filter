# iPhone Call Directory probe

Two separate uses: the silent-blocking device protocol below, and the historical
capacity measurements. This is not the production app or its rule editor.

**This exists because the limit is undocumented and may differ by device.** Only
one device has been measured. Re-run this on any other iPhone or iOS version
before trusting the number there.

## Result so far

iPhone 13 (iPhone14,5), iOS 26.6.1 — blocking entries, full reload:

| entries | result |
|---:|---|
| 1,000 | OK 0.05s |
| 1,000,000 | OK 7.56s |
| 1,500,000 | OK 9.40s |
| 1,750,000 | OK 13.00s |
| 1,900,000 | OK 12.11s |
| 2,000,000 | **FAIL** — error 5, MaximumEntriesExceeded |
| 5,000,000 | FAIL — error 5, after 23.96s |

Identification entries (number + label): 1,900,000 OK in **47.58s** — roughly 4x
the cost of plain blocking entries for the same count.

So the cap sits between 1.9M and 2M on this device. Throughput ≈ 130k–160k
entries/second.

## Prepare the silent-blocking probe

Run commands from the repository root. Requires macOS, Xcode with the iPhoneOS
SDK, XcodeGen, Swift and Python 3.9+. No Android SDK, Rust rebuild, App Group or
paid-team feature is required by this bundled-fixture design.

```sh
bash tools/device-tests/test-ios-probe.sh
bash tools/entry-limit-harness/run.sh build --mode empty --alias control
```

The default is an **unsigned Release build for physical iPhone**, not an
installable signed app. Nothing is installed or launched. The script prints the
app path and private log. Signing/provisioning and device execution remain
unverified until the selected phone is connected.

For exact blocking, place one **already validated canonical international
number** (`+` followed by digits, no formatting or extension) in a private text
file. Do not put the number on a command line or in tracked source. Use a private
editor, and a non-personal alias that distinguishes this fixture from others.

```sh
mkdir -p target/device-tests/my-run
chmod 700 target/device-tests/my-run
# Privately create target/device-tests/my-run/caller.txt with the validated number.
bash tools/entry-limit-harness/run.sh build --mode exact --alias caller-a \
  --number-file target/device-tests/my-run/caller.txt
```

The Swift code checks transport shape and integer overflow only. It does not
validate a numbering plan or normalize national input. That remains Rust's job;
this probe deliberately takes a prevalidated fixture. Confirm its number against
the caller's presentation before trials. Synthetic numbers used by host tests
must never be used for physical calls.

Each build stages source, generated project/plist, the fixture and products in a
new directory under `target/device-tests/ios/`. The source checkout and tracked
`CallDir/Info.plist` are not rewritten by XcodeGen. Identical fixture copies are
checked in the app and extension. `build-record.json` records revision, dirty
state, command, artifact hashes and signing status. It is not physical evidence.
Exact mode emits one blocking entry and no label; empty mode emits none. Both
clear old blocking and identification entries on incremental requests. Missing
or malformed data is an error, never a fallback to capacity entries.

These ignored directories are private, not encrypted or backed up. Bundles also
contain the number; do not upload apps or raw build logs as public artifacts.
Archive agreed evidence before `cargo clean`. See the [protocol](../device-tests/README.md)
and its [results template](../device-tests/results-template.md).

## Sign, install and enable — only with the user's consent

Set `DEVELOPMENT_TEAM` in ignored `apps/ios/.env` or the environment. Set
`IOS_DEVICE` locally to the **explicitly chosen connected device identifier**.
The script never selects the first phone, registers devices or requests
provisioning updates automatically. A usable signing certificate/profile,
pairing, Developer Mode and developer trust may require action in Xcode/on the
phone. Resolve those with the user; a free-team setup is not assumed to work
without those checks.

Rebuild both fixtures for that device, adding `--sign --device "$IOS_DEVICE"`:

```sh
bash tools/entry-limit-harness/run.sh build --mode empty --alias control \
  --sign --device "$IOS_DEVICE"
bash tools/entry-limit-harness/run.sh build --mode exact --alias caller-a \
  --number-file target/device-tests/my-run/caller.txt --sign --device "$IOS_DEVICE"
# Save the printed, signed paths as EMPTY_APP and DENY_APP in your local shell.
bash tools/entry-limit-harness/run.sh install --app "$EMPTY_APP" --device "$IOS_DEVICE"
```

Installation is a separate command. It verifies both bundle identifiers,
matching fixtures, embedded provisioning and the code signature before calling
`devicectl`. It does not launch the app or dial. Unsigned preparation builds are
rejected. Use the printed signed paths, not a generic DerivedData search.

1. Enable **LimitTest** under **Settings → Apps → Phone → Call Blocking &
   Identification** (wording depends on iOS). Complete developer trust if needed.
2. Open **LimitTest**. Check the fixture alias/mode and actual extension state.
3. Press **Reload bundled list**. Wait for reload success **and** enabled status.
   Unknown, disabled, loading or failed is not ready. Settings/status controls
   are included. Exact/empty modes never auto-reload on app launch.
4. Follow the protocol's empty baseline, deny and empty recovery. Switch with the
   explicit install command using `DENY_APP` or `EMPTY_APP`, then open/reload and
   check status again. Both have the same bundle ID; installing is not proof that
   iOS has replaced its old list. No calls during setup/reload.
5. Record the physical observations separately. Reload success only means iOS
   accepted a list. There is no blocked-call callback, history or custom alert.

The app saves timestamped **reload** results in `Documents/results.txt`, available
through Finder file sharing. Console logs use subsystem
`com.antoniopantano.limittest`. They omit the number and raw OS error descriptions;
raw system logs/recordings may still contain personal data and stay private.
Record the fixture alias/build path alongside exported results.

## Clear and clean up

Install `EMPTY_APP`, open/reload, verify enabled + success, and complete the
protocol's ringing recovery. If setup is failing, disable this extension in
Phone settings; this is an emergency cleanup route, not proof of an empty reload.
Do not assume uninstall clears OS-held entries. Uninstall only after cleanup,
with approval, using the selected phone's normal app removal or:

```sh
xcrun devicectl device uninstall app --device "$IOS_DEVICE" com.antoniopantano.limittest
```

Save/redact agreed evidence before deleting that run's ignored files. Restore
only settings changed with consent; leave unrelated calls, messages and rules alone.

## Historical capacity modes

```sh
bash tools/entry-limit-harness/run.sh 1500000 blocking
# Or identification; append --sign --device "$IOS_DEVICE" for a signed build.
```

Legacy numeric arguments still generate the same synthetic ascending ranges.
**The script now builds only**; install explicitly as above. Capacity modes
auto-reload on launch once enabled, and retain bounded retries for error 7.
`harness-defaults.json` contains the synthetic base, progress interval and retry
settings; these are harness controls, not phone-country facts or measured OS
limits. Capacity modes are not the silent-blocking protocol. Only physical runs
supply meaningful capacity/footprint measurements; do not reinterpret the old
measurements as verification of this new code.

## Error codes you will see

| code | meaning |
|---|---|
| 5 | MaximumEntriesExceeded — the answer you are looking for |
| 6 | ExtensionDisabled — turn it on in Settings |
| 7 | CurrentlyLoading — normal after install or during a big load; the app retries |

## Four things that will waste your time if you don't know them

1. **Build Release.** Xcode 26 Debug builds put target code in a separate
   `__preview.dylib`, so the extension's principal class is missing from the
   `.appex` and enabling it fails with a generic error. `run.sh` already does this.
2. **Honour `context.isIncremental`.** iOS often asks for an incremental load.
   Re-adding numbers it already holds fails with sqlite error 19, surfaced as a
   raw `INSERT INTO PhoneNumberBlockingEntry` error. The handler clears first.
3. **Entries must be strictly ascending** or the whole request fails. Capacity
   fixtures validate integer bounds before streaming consecutive entries;
   exact fixtures contain only one entry. Host tests check both paths.
4. **xcodegen's `info.path` generates a plist**, silently dropping a hand-written
   `NSExtension` dictionary. Keys go under `info.properties`.

## Why the number matters

It decides, for every country, how many digits a user must pin down before a rule
is accepted. But the app must never hardcode it — error 5 is catchable, so the
real app attempts the load and backs off. This harness tells us what to *expect*
and what to warn about, not what to rely on.

This project is throwaway and separate from `apps/ios`. Its target name carries
no decision.
