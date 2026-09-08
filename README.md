# Caller Filter

A call and SMS filtering app for Android and iOS, built around rules the user
writes themselves rather than a fixed spam list.

The user writes a rule — an exact number, a pattern, a prefix, a city — and the
app applies it to incoming calls and messages. Where a platform cannot honour a
rule, the app says so, with the real number behind the limit.

## Repository layout

**One repository holding the core and both apps.**

```
core/        Rust filtering engine. All logic lives here.
tools/       Dataset converter and build tooling.
docs/        Design notes that belong next to the code.
```

The iOS and Android projects join this tree in the next task.

### Why one repository

The apps are thin shells over the core, and all filtering logic is in the core
by design. So the core changes far more often than the shells, and a core change
plus its regenerated bindings should land in a single commit.

Separate repositories would need a publishing step for the core, and would let
an app sit silently on a stale version of it. The cost of one repository —
iOS and Android developers carry the whole tree — is small by comparison.

## Two rules that shape the code

**Nothing country-specific in a source file.** No country, country code, prefix,
number length or place name. All of it is data loaded at runtime, so adding a
country means shipping data, not changing code and not making a release.

**The rule is the truth; expansions are derived.** iOS cannot evaluate anything
when a call arrives, so rules are expanded ahead of time into a list of exact
numbers. That list is a build product of the rule. Store the rule; never store
the expansion, or a data update will silently rot every rule the user wrote.

The full set is in the planner under Project Guidelines.

## What is never committed

- **Generated UniFFI bindings.** They drift from the core in silence and fail at
  runtime on one platform only.
- **Packaged datasets.** Committing one defeats over-the-air data versioning, and
  a silently truncated dataset makes rules stop matching with no error anywhere.

Both are build outputs. See `.gitignore`.

## Building

```sh
cargo test          # core logic, host only, no device or simulator needed
```

The core is pure logic with no platform dependency. Keep it that way: its tests
are the fast feedback loop for every platform at once.

## Measured platform facts

These were established by reading the SDKs and by measuring on a real device,
not from documentation. They are recorded in full in the planner.

| | |
|---|---|
| iOS Call Directory cap | ~2,000,000 entries (measured, iPhone 13 / iOS 26.6.1) |
| iOS call-time evaluation | not possible — rules must be pre-expanded |
| iOS SMS | evaluates live; strongest verdict is the Junk folder |
| Android call screening | evaluates live, **5 second budget** before the phone rings |
| Android SMS blocking | requires being the default SMS app — out of scope |

## Planning

Feature, phase, task and requirement state lives in `.planner/`. Read it before
changing anything structural; the reasoning behind most of the decisions here is
recorded there rather than in commit messages.
