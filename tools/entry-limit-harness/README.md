# iOS Call Directory entry-limit harness

Measures how many entries the iOS Call Directory will accept before it refuses.

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

## Running it

Needs `DEVELOPMENT_TEAM` in `apps/ios/.env`, and a real device — the simulator
gives no meaningful memory or timing.

```sh
./run.sh 1500000 blocking          # or: identification
```

Then open **LimitTest** on the phone. It runs automatically and shows the result.

First run only: enable the extension in
**Settings → Apps → Phone → Call Blocking & Identification**, and trust the
developer certificate under **Settings → General → VPN & Device Management**.
Until then `reloadExtension` returns error 6.

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
3. **Entries must be strictly ascending** or the whole request fails. The
   generator asserts monotonicity before measuring.
4. **xcodegen's `info.path` generates a plist**, silently dropping a hand-written
   `NSExtension` dictionary. Keys go under `info.properties`.

## Why the number matters

It decides, for every country, how many digits a user must pin down before a rule
is accepted. But the app must never hardcode it — error 5 is catchable, so the
real app attempts the load and backs off. This harness tells us what to *expect*
and what to warn about, not what to rely on.

This project is throwaway and separate from `apps/ios`. Its target name carries
no decision.
