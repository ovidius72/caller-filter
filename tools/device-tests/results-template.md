# Silent-blocking result — <run alias / platform>

**Initial status: not-tested.** Copy this template into the private run directory.
Follow `tools/device-tests/README.md` in the repository, including when this
result is copied to a private run directory. Leave no required field implicitly verified.
Use aliases; real numbers, UDIDs, signing identities and raw recordings stay private.

## Run and permission

| Field | Recorded value |
|---|---|
| Run / receiving-phone / caller aliases | <fill> |
| Observer and date/timezone | <fill> |
| User consent: installation, settings changes, calling session | <scope and time; pending until given> |
| Platform task | <T022 Android or T023 iPhone> |
| Device model / OS version and build | <fill> |
| Receiving carrier / SIM-line alias / caller carrier if known | <fill or unknown> |
| Dialer / cellular or Wi-Fi calling / signal | <fill or unknown> |
| Artifact identity / source revision / dirty-worktree state | <fill; revision alone is insufficient> |
| Exact build/install commands and private build-log path | <fill; redact identifiers before sharing> |
| Private fixture location and alias; presented number matches | <location and verification; never copy the number here> |
| Role / extension enabled state and loading evidence | <fill; unknown/failed is not ready> |
| Observer/phone clock alignment and evidence location | <fill> |

## Baseline and planned settings

Record before/after values and permission for each change. Unknown is not off.

| Setting | Baseline / approved test value / restoration needed |
|---|---|
| Ring volume / silent mode / vibration | <fill> |
| Focus / Do Not Disturb | <fill> |
| Native block list / unknown-spam screening / other blockers | <fill> |
| App/system notification settings | <fill; not globally disabled to force silence> |
| Android contact eligibility / screening role | <fill or not applicable> |
| iPhone extension enablement / loaded fixture | <fill or not applicable> |
| Battery restrictions / lifecycle state | <fill or unknown> |
| Watch/headset/other alert routing | <disconnected with consent, or observed> |
| Known operator missed-call services | <fill or unknown; no unapproved changes> |

| Test parameter | Chosen before calls | Actual deviations and reasons |
|---|---|---|
| Maximum attempt duration, seconds | <fill> | <fill or none> |
| Observation after each call, seconds | <fill> | <fill or none> |
| Final observation, seconds | <fill> | <fill or none> |
| Denied attempts per state | <fill> | <fill or none> |

These windows bound the conclusion; they are not known maximum delivery delays.

## Case matrix

S1 = unlocked/foreground; S2 = unlocked/background; S3 = locked/not visible.
M0 = deliberately missed empty-fixture control, performed first to check normal
notification delivery. A = answered empty control; B = exact deny; C = recovery.
This starting matrix has two B attempts per state; adjust it before calls. Keep
omitted or aborted attempts visible with their reason. Never overwrite a failed
attempt with its successful retry.

| Case | State | Fixture / expected behavior | Result | Attempt evidence |
|---|---|---|---|---|
| M0 | S2 | Empty / deliberately missed; record normal notification and operator alerts | not-tested | <fill> |
| S1-A | S1 | Empty / rings, vibrates, incoming UI | not-tested | <fill> |
| S1-B1 | S1 | Exact deny / no disturbance | not-tested | <fill> |
| S1-B2 | S1 | Exact deny / no disturbance | not-tested | <fill> |
| S1-C | S1 | Empty / normal calling returns | not-tested | <fill> |
| S2-A | S2 | Empty / rings, vibrates, incoming UI | not-tested | <fill> |
| S2-B1 | S2 | Exact deny / no disturbance | not-tested | <fill> |
| S2-B2 | S2 | Exact deny / no disturbance | not-tested | <fill> |
| S2-C | S2 | Empty / normal calling returns | not-tested | <fill> |
| S3-A | S3 | Empty / rings, vibrates, incoming UI | not-tested | <fill> |
| S3-B1 | S3 | Exact deny / no disturbance | not-tested | <fill> |
| S3-B2 | S3 | Exact deny / no disturbance | not-tested | <fill> |
| S3-C | S3 | Empty / normal calling returns | not-tested | <fill> |
| N (optional) | <record> | Deny active, different caller / rings | not-tested | <reason if unavailable> |

If N is not tested, do not claim demonstrated selectivity between two real callers.
Extra lifecycle cases (force-stop, reboot, etc.) must be named and recorded separately.

## Attempt record — duplicate for every call

**Case / attempt ID:** <fill>

| Observation | Value and evidence |
|---|---|
| Fixture/mode ready before call; screen/app/process state | <fill; process may be unknown> |
| Dial start / end / termination reason / actual duration | <timestamps with timezone; include caller-side context> |
| Post-call observation start / end | <timestamps; note interruptions> |
| Ring sound | <present / absent / unobserved, evidence> |
| Vibration | <present / absent / unobserved, evidence> |
| Incoming screen/banner | <present / absent / unobserved, evidence> |
| Blocked/missed-call notification, including notification center | <present / absent / unobserved, evidence> |
| Custom app alert/overlay | <present / absent / unobserved, evidence> |
| Connected-accessory alert | <present / absent / unobserved / not applicable> |
| Call record | <present / absent / unobserved; a record is allowed> |
| Delayed SMS/other alert | <none observed in window, or alert-log ID> |
| Android incoming invocation / core verdict / response flags | <evidence or gap; not an outgoing callback> |
| Android binding-to-response and callback-to-response timing | <durations and measurement points; unknown where unavailable> |
| iPhone enabled state / reload outcome / fixture alias | <evidence; never treated as a per-call callback> |
| Matching controls valid; required channels observed | <case IDs and gaps> |
| Result and reason | <pass / fail / blocked / not-tested; evidence references> |

Screen video alone does not prove sound/vibration. Record the observer's evidence.
A failed control or missing observation prevents an unconditional silence pass.
Without a demonstrated M0 notification, missed-notification suppression remains
unverified. Distinguish baseline alerts from the denied-call windows.

## Delayed alert log and final window

| Alert ID / time | Related case / channel / sender alias | Source and evidence | Disturbance / unresolved questions |
|---|---|---|---|
| <fill or none observed> | <fill> | <app / system / operator / unknown; explain attribution> | <fill> |

- Final observation start/end and actual duration: <fill>
- Alerts arriving after that window: <fill or none known; reopen verdict if needed>
- Any unobserved interval or ambiguous call association: <fill or none>
- Proposed operator/system changes: <none or proposal awaiting user approval>

## Verdict and recovery

- App/system no-disturbance result: **not-tested** — <evidence and scope>
- Operator/unknown alerts: **not-tested** — <observed, absent within window, or unknown>
- Overall result: **not-tested** — <pass / fail / blocked / not-tested; reasons>
- Conditional behavior, failures and unverified cases: <fill; no silent exceptions>
- Empty-fixture recovery evidence in all states: <case IDs>
- Test blocks/fixtures removed and OS state checked: <evidence or pending>
- Settings restored with consent; private artifacts retained/removed as agreed: <fill>
- Cleanup failures or remaining device changes: <fill or none>
- Redacted summary / private evidence references: <fill>

`blocked` means the test cannot establish a result, not that a call was denied.
Residual disturbance or unresolved attribution is not an unconditional pass.

## Cross-platform review — completed by the second physical task

| Platform | Run reference / tested configuration | Verdict / unresolved conditions |
|---|---|---|
| Android | <fill> | not-tested |
| iPhone | <fill> | not-tested |

- User go/no-go after reviewing both results: **pending**.
- Later production-integration rerun required: **yes**.
- Do not proceed to other product work on the strength of an incomplete result.
