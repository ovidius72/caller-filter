# Silent-blocking device tests

T019 defines this protocol. T020/T021 prepare the iPhone/Android probes;
T022/T023 run them with the user's physical phones and permission.
**This document contains no physical test results.**

## Acceptance

A denied cellular call must cause no ringtone, vibration, incoming-call screen
or banner, blocked/missed-call notification, or notification-center entry.
Our app must not generate an alert or overlay of its own. A Phone/Recents
record is allowed.

Record delayed SMS and other alerts separately. An operator-generated message
still prevents an unconditional no-disturbance pass, but is not automatically
an app defect. Record unknown origins as unknown. Do not change operator
services without consent or add voicemail settings/SMS-filter workarounds.
A pass covers only the tested configuration and observation window.

## What is already known

- The [iPhone probe](../entry-limit-harness/README.md) now supports private exact
  and empty fixtures, explicit device selection and separate build/install steps.
  Its historical capacity measurements establish list loading, not silent calls.
  Use the exact/empty modes for this protocol, not synthetic capacity entries.
- The [Android probe](android-probe.md) supplies a screening service, role and
  exact/empty controls plus an APK build/verification helper. Its native startup,
  role acquisition and physical silent-blocking behavior still need device tests.
- [Apple Call Directory](https://developer.apple.com/documentation/callkit/identifying-and-blocking-calls)
  loads exact numbers in advance. The app receives no blocked-call event or
  call history. A successful reload proves loading, not silence.
- [Android screening](https://developer.android.com/reference/android/telecom/CallScreeningService)
  requires a response within five seconds of binding. Use a visible caller
  number outside contacts for this probe; do not require READ_CONTACTS.
- [Android response controls](https://developer.android.com/reference/android/telecom/CallScreeningService.CallResponse.Builder)
  distinguish blocking, silencing and notification suppression. Silencing alone
  is insufficient. Ordinary third-party apps cannot rely on `setSkipCallLog`;
  call records are permitted here.

Android facts were checked against SDK-36 `CallScreeningService.java` lifecycle
and builder documentation. The older T002 completion and User-defined filtering
requirement overstate call-log suppression; do not reproduce that assumption.
SDK/source and host-test evidence do not establish physical silent blocking.

## Private fixtures and evidence

Copy [results-template.md](results-template.md) for each receiving phone into
`target/device-tests/<run-alias>/`. This directory is already Git-ignored.
Store real numbers, device identifiers, raw logs and recordings there or in an
explicitly chosen private location. Use aliases in shareable reports. Redact
numbers, UDIDs, signing identities and unrelated notifications before sharing.
Ignored does not mean encrypted or backed up; preserve evidence before
`cargo clean` and agree retention/deletion with the user.

Hand these values to both probe tasks; their transport is not a new production
rule-storage format:

| Input | Contract |
|---|---|
| Run/fixture/caller aliases | Non-personal labels for logs and reports. |
| Private caller number | One already validated international number: `+` and digits; no national-input guessing. |
| Fixture mode | Empty rule set, or one exact deny for that caller. |
| Target device | Explicit local selection; never a committed UDID or arbitrary first device. |
| Build identity | Revision, dirty-worktree state, artifact identity and actual build command; record rebuilds. |

Keep the authored rule separate from the derived iOS entry. Validation/matching
belongs in Rust, not duplicated in Swift/Kotlin. Invalid or absent fixtures must
show a setup error, never silently select synthetic or unrelated numbers.

## Probe handoff and readiness

**T020 — iPhone:** package a private exact-number fixture and an empty-list
control; rebuilding a bundled fixture is sufficient. App Groups/editable shared
storage are not prerequisites. The test mode emits blocking entries, not labels,
and clears stale blocking/identification entries on incremental loads. Preserve
the old capacity modes and known Release/`ENABLE_DEBUG_DYLIB=NO` build fix.
Report extension status and reload errors. Disabled, unknown, loading or failed
is not ready. Supply explicit build/install/clear/cleanup commands and signing
requirements; device-bound provisioning and trust remain pending until connection.

**T021 — Android:** supply an APK with the required native ABI and fresh bindings,
a declared screening service, role request/status and fixture controls. Use the
existing Rust exact matcher; deny with disallow and missed-call notification
suppression, documenting the reject setting. Empty/nonmatching rules allow.
No overlay, custom call notification, default-dialer/default-SMS status or
call-log/contact permission is needed. Record incoming invocation, core verdict,
response flags and monotonic timing. Outgoing callbacks are not blocking evidence.
Callback-to-response timing alone does not prove the binding-to-response deadline;
record binding evidence when observable and identify any measurement gap.

Before asking the user to connect phones, both probe tasks must provide:

- successful applicable builds/focused checks, artifact paths and exact commands;
- empty/deny controls, active fixture/status diagnostics and private log capture;
- explicit device selection, installation, clearing and cleanup instructions;
- a list of remaining pairing, trust, role/extension and runtime checks.

T019 alone does not make the probes ready. Installations and calls require the
user's consent; do not contact unrelated or emergency numbers.

## Setup for each receiving phone

Two consented cellular lines suffice; the two phones may call each other in
separate runs. A third phone is not required. Record handset, OS/build, carrier,
line alias, dialer, artifact, transport (including Wi-Fi calling if known),
clock/timezone and unknowns in the template.

Record settings before changing anything: volume, silent mode, vibration,
Focus/Do Not Disturb, native block lists, unknown/spam screening, other blockers,
notification settings, battery restrictions, connected accessories and known
operator missed-call services. Obtain approval for changes and later restoration.
Do not globally suppress notifications to manufacture a pass. Disconnect watches
or headsets with permission, or observe them too. Do not silently delete contacts.

Confirm the presented caller number equals the private fixture. On Android,
withheld/unknown/unavailable/payphone presentations cannot test this screening
path. Verify the role or extension is enabled and the selected fixture is ready.

Record test parameters before calling. Suggested starting values are **test
settings, not verified OS/operator delay limits**: attempt at most 30 seconds if
the network has not ended it; observe for 60 seconds after each call and five
minutes after the final call; repeat each denied case twice. Record chosen values
and actual times. Increase windows for known delays. Later alerts reopen results;
do not shorten windows after a failure or silently omit repeats.

## Required matrix

First run **M0 — missed-call control**, once per phone with an empty fixture and
an unlocked/background state. Let the consented call ring without answering,
end it, and observe the full post-call window. Record the normal missed-call
notification and any operator message. This checks that notification suppression
is not merely a global setting. If no baseline notification can be demonstrated,
mark that channel unverified. Distinguish its timestamps from later denied calls;
ambiguous delayed alerts cannot support a pass. Do not clear them without consent.

Then run the state matrix below. A/C controls are answered to avoid generating
further baseline missed-call alerts.

| State | Screen | Probe UI |
|---|---|---|
| S1 | Unlocked | Foreground and visible |
| S2 | Unlocked | Background; another ordinary app visible |
| S3 | Locked | Not visible; phone left idle |

Background/not visible is not Android Settings → Force stop. Reboot, force-stop
and other lifecycle variants are separately named extra cases. Record process
state if known; dismissing an app does not prove its process exited.

For **each state**, with other conditions unchanged:

1. **A — control:** activate and verify the empty fixture, enter the state and
   call. Observe audible ringing, vibration and incoming UI, then answer/end.
   Answering avoids creating a baseline missed-call alert.
2. **B — deny:** activate/verify the exact deny and restore the same state after
   setup. Make the configured attempts. Observe from before dialling through
   termination and the complete post-call window. Record unexpected UI/alerts
   as failures; do not dismiss them to make the block look silent. Record early
   network termination, actual duration and each attempt separately.
3. **C — recovery:** clear/verify the empty fixture, restore the state and confirm
   the same caller rings again. Answer/end and complete the observation window.

A non-ringing control prevents a silence conclusion: mark the sequence `blocked`
until corrected and repeated. An unobserved channel remains unverified. Stop
calls when consent is withdrawn; record setup changes and rerun affected controls.

If another consented caller line is available, add **N — nonmatching caller while
B is active** and verify ringing. Otherwise record N as `not-tested`; this minimal
run then does not prove selectivity between two actual caller numbers. N is not
a prerequisite requiring another device.

## Observation, verdict and cleanup

Record sound, vibration, UI and notification-center/app alerts separately. A
screen recording alone cannot prove no sound or vibration; add an observer or
external evidence. Distinguish pre-existing notifications using timestamps;
do not delete history without consent. For delayed SMS/alerts record time,
channel, sender alias, call association and evidenced source: app, system,
operator or unknown. Caller-side busy/voicemail audio is context, not proof of
what the receiving phone did.

Use `pass` (complete, valid observations meet expectations), `fail` (observed
violation), `blocked` (setup/evidence prevents a conclusion), or `not-tested`
(no attempt). Here `blocked` is a **test status**, not the core deny verdict.
Report app/system behavior separately from operator/unknown alerts. Unresolved
attribution or a residual disturbance cannot yield an unconditional overall pass;
present conditions to the user without weakening acceptance.

After recovery and final observation, clear test fixtures/blocks, restore agreed
settings and record cleanup. Do not assume uninstall removed OS-held entries or
delete unrelated rules, messages or logs. Finish the result template with evidence
and gaps; absence of hardware is not a pass. The second physical task to finish
consolidates both platform results and **stops for the user's go/no-go**.

These tests do not validate partial-number entry, AND/OR, geography, capacity or
production integration. Repeat accepted cases when the real apps are integrated.
