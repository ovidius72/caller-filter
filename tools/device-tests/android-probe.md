# Android silent-blocking probe

Preparation for the [physical protocol](README.md), not production integration
or evidence that a phone stays silent. No install or call is part of a build.
The diagnostic UI is English-only; production localization is out of scope.

## Requirements and build

Use an installed SDK (platform 36 and build-tools), NDK, cargo-ndk, Android Rust
targets, Gradle and JDK 17 or newer, plus Python 3.9+. Set `ANDROID_HOME` (or
`ANDROID_SDK_ROOT`); the local fallback is the Homebrew SDK location. The native
builder selects an installed NDK unless `ANDROID_NDK_HOME` is supplied. It must
not be run concurrently with another Android build in this checkout because it
replaces `apps/android/app/generated/`.

From the repository root:

```sh
python3 tools/device-tests/android-probe.py build --mode empty --alias control
```

For a real trial, store **one prevalidated international caller number**, with
`+` and digits, in an ignored private text file such as
`target/device-tests/input/caller-number.txt`. Use a private editor/file, not a
number embedded in a shell command or committed source. Use `umask 077` when
creating private input/evidence files. Supply no comments, spaces or other text.
Then build:

```sh
python3 tools/device-tests/android-probe.py build \
  --mode exact --alias consented-caller \
  --number-file target/device-tests/input/caller-number.txt
```

Builds are offline by default. Add `--online` if Gradle dependencies are missing
from the cache; this permits Maven dependency downloads, **not SDK installation**.
The helper regenerates all native libraries and bindings, validates the actual
private fixture through Rust, runs focused host/tooling tests, assembles a debug
APK and runs Android Lint. It checks the bundled fixture, service/native inventory,
manifest/permissions, debug signature, ZIP alignment and 64-bit ELF alignment.
JNA 5.17 and both 64-bit core libraries support 16 KB alignment. This packaging
check does not prove native startup on a real phone.

The helper prints a private run directory under `target/device-tests/android/`
containing `caller-filter-probe.apk`, `build-record.json`, fixture, manifest and
logs. The record includes artifact SHA-256, revision/dirty state and commands.
APKs include arm64-v8a, armeabi-v7a and x86_64 and use the local **development
key**, not a distribution signature. Do not commit APKs, generated bindings,
private numbers, device IDs or logs. An APK contains the fixture in readable form;
Git-ignore and restrictive local permissions are not encryption.

Host synthetic fixtures are not callable numbers and cannot test real blocking.
Rust's exact-matcher shape check is not numbering-plan/assignment validation.
The operator must confirm the real caller's already validated number and consent.
A new real fixture requires a new APK, not a source edit.

## Installation — only after consent and connection

1. Connect/unlock the chosen Android phone, enable USB debugging with permission
   and accept its computer-trust prompt. Set `ANDROID_SERIAL` to that device's
   exact serial; never select an arbitrary first device. Verify Android 10+ and
   an available call-screening role. The APK minimum remains Android 8, but the
   probe will explicitly report the role unavailable on older versions.
2. Set `APK` to the **verified private APK path printed by the builder**:

   ```sh
   python3 tools/device-tests/android-probe.py install \
     --serial "$ANDROID_SERIAL" --apk "$APK"
   ```

   This verifies the recorded hash and installs only. It does not launch the app,
   request a role or make a call. It replaces the existing debug app with package
   `com.antoniopantano.callerfilter`; confirm that replacement is acceptable.
   Signature conflicts must be resolved with the user, not by silently uninstalling.
3. Open **Caller Filter Probe** manually. Grant the **call-screening** role using
   the button and system dialog. Do not grant default-dialer/default-SMS roles,
   contacts, call-log or notification permissions. The probe requests none of them.
4. Wait for Rust/fixture readiness and confirm the actual role says `HELD`.
   Role state refreshes on resume and on request. Revocation/not granted is not
   ready. Setup failure is visible and permits calls; it is not a blocking pass.

No overlay or app-generated notification is used. A `Ready` label means only
local role/core/fixture setup, never verified silence or an OS acknowledgement.

## Empty / deny / recovery controls

An exact-fixture APK also supplies the empty control; rebuilding for each call
is unnecessary. A first installation or changed fixture starts empty. Activation
of an identical fixture persists across process recreation and APK replacement,
so **always explicitly select and verify the empty control before M0/A**.
Changing the packaged fixture invalidates the previous selection.

- **M0/A:** tap **Use empty control / clear deny**. Wait for `empty control` and
  readiness before leaving the app or calling.
- **B:** tap **Activate bundled exact deny**. Wait for `exact deny` and readiness.
  Restore the required foreground/background/locked state, then run the attempt.
- **C:** select the empty control again, wait, and verify the same caller rings.
- A nonmatching canonical caller is allowed even while B is active.
- If selection/clearing fails, stop trials and revoke the screening role through
  system settings. Do not assume a failed preference write persisted a clear.

Matching is exclusively existing Rust `PreparedRules`/`evaluateNumber`; Kotlin
only removes the fixture's `+` transport marker and adapts the verdict. This
probe accepts canonical international `tel:` handles. It does not guess a
country or normalize national-format caller IDs. `UNSUPPORTED_HANDLE` means the
path was not tested successfully; it must not be reported as a block.

For a Rust deny the service sends `disallow=true`, `reject=true`,
`skipNotification=true`, `skipCallLog=false`. Rejection asks Telecom to disconnect
as though manually rejected, rather than merely silence a still-visible call.
It does not guarantee carrier handling or absence of operator-generated SMS.
Call records are allowed; third-party `setSkipCallLog` cannot be relied upon.
Every non-deny/error/deadline outcome sends allow flags. Outgoing callbacks receive
no blocking response. No call-history access or blocked-call statistics are added.

## Diagnostics and timing

Before the matrix, start a dedicated capture; `RUN` must be a private path under
`target/device-tests/` and the destination must not already exist:

```sh
python3 tools/device-tests/android-probe.py logs \
  --serial "$ANDROID_SERIAL" --output "$RUN/android-probe.log"
```

Stop capture with Ctrl-C. It does not clear logcat or overwrite existing evidence.
The app logs `CallerFilterProbe` JSON with UTC/monotonic timestamps, request IDs,
reason, matched rule ID and response flags; **no phone number or raw call handle**.
The last event is also visible after refreshing the app's diagnostics. Capture
also includes `AndroidRuntime` errors, potentially from other apps: treat raw
logs as private and redact unrelated data before sharing.

`api_returned` only means `respondToCall` returned. SDK code can swallow a remote
error; `framework_acceptance=unverified` is intentional. Correlate each request
with physical observation. Missing invocation, `api_error`, `DEADLINE`, unavailable
core or unexpected verdict cannot count as successful blocking.

Native initialization/evaluation and preference writes run on one serial worker,
not the screening callback thread. A main-thread watchdog permits the call at
the configured local budget, ignoring later native replies; pending replies are
cancelled at unbind. `probe_response_budget_ms` in the resource file currently
sets 3000 ms, leaving headroom under the SDK's five-second contract. This is a
**test setting, not a measured hardware guarantee**. Main-thread/OS scheduling
and cold process startup still require device measurement.

For the first request after a local `onBind`, diagnostics include
`local_bind_to_return_ms`. A reused binding has no new per-call local bind
observation: the value is `-1`, and `bind_observed_for_request=false` marks the
gap. `callback_to_return_ms` excludes earlier startup/binding time. Neither
measurement alone proves the full framework deadline. Record that limitation
and obtain stronger system evidence if needed; do not silently relabel it.

## Clearing and cleanup

After the final recovery/control and observation window, select empty and revoke
the screening role through **Open default-app settings**. Restore only settings
whose changes were approved and record cleanup in the results template. Clearing
the deny does **not** erase the private fixture still bundled inside the APK.
To remove it, build/install an empty APK or uninstall with the user's permission:

```sh
"$ANDROID_HOME/platform-tools/adb" -s "$ANDROID_SERIAL" uninstall \
  com.antoniopantano.callerfilter
```

Do not automatically delete phone history, logs or recordings. Agree retention
of private builds, inputs and evidence before deleting them or running `cargo clean`.

## Verification boundary

Host tests cover real Rust exact/nonmatch/empty/recovery behavior, invalid fixture
handling, response mapping, one-response/deadline/cancellation guards and the
actual packaged input. Python tests exercise private staging, ELF checks, APK
inventory, explicit-device mocked install and non-destructive private log capture.
They do **not** execute a real Android `CallScreeningService`, acquire a role,
measure its real deadline or observe sound, vibration, UI, notifications or carrier
messages. All those remain physical-test work. Production app integration must
repeat accepted cases; partial-number authoring and geographic filters are separate.
