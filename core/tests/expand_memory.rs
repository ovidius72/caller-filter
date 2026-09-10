//! Proof that expanding a rule set does not hold the list in memory.
//!
//! This has its own test binary because it installs a global allocator, and a
//! process gets one of those. It counts live bytes rather than arguing from the
//! shape of the code — the previous version of `expand_rules` looked fine and
//! collected millions of numbers into a Vec.
//!
//! Why it matters: the iOS Call Directory extension is chosen by the system for
//! its tight memory limit, and when it is killed the app is told nothing. A
//! buffer that is merely large enough usually works, and fails on the phone
//! belonging to the user with the most rules.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::sync::Mutex;

use callerfilter_core::{
    expand_rules, expand_rules_to_vec, Budget, Effect, Matcher, Pattern, Rule, RuleId, RuleSet,
};
use phonenumber::metadata::DATABASE;

// Signed, because memory allocated before a measurement started is still
// freed during it. An unsigned counter underflows on the first such free and
// reports a peak of nearly u64::MAX.
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static WATCHING: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && WATCHING.load(Ordering::Relaxed) == 1 {
            let live =
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed) + layout.size() as isize;
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if WATCHING.load(Ordering::Relaxed) == 1 {
            LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        }
        unsafe { System.dealloc(p, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Only one measurement at a time. The counters are global, and the test
/// harness runs tests in parallel threads, so two overlapping measurements
/// would each report the other's allocations.
static MEASURING: Mutex<()> = Mutex::new(());

/// Run `body` with allocation accounting on, and report the peak live bytes.
fn peak_bytes(body: impl FnOnce()) -> usize {
    let _guard = MEASURING.lock().unwrap_or_else(|e| e.into_inner());
    LIVE.store(0, Ordering::SeqCst);
    PEAK.store(0, Ordering::SeqCst);
    WATCHING.store(1, Ordering::SeqCst);
    body();
    WATCHING.store(0, Ordering::SeqCst);
    PEAK.load(Ordering::SeqCst).max(0) as usize
}

fn budget(entries: u64) -> Budget {
    Budget {
        max_entries: entries,
        max_candidates: entries.saturating_mul(10),
    }
}

/// A rule set big enough that a buffer would dominate, small enough that the
/// debug build stays quick — this runs on every CI push.
fn wide_rules() -> RuleSet {
    RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("39021234xxxx").expect("pattern")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("390212345xxx").expect("pattern")),
        ),
    ])
}

#[test]
fn streaming_costs_far_less_than_collecting_the_same_list() {
    // The two paths, same rules, same numbers, measured against each other.
    // A ratio rather than a byte count, because a fixed threshold would really
    // be measuring the metadata regex caches warming up — those are a constant
    // that a bigger expansion would hide and a smaller one would fail on.
    let rules = wide_rules();

    // Warm the metadata regex caches first. They are populated lazily on first
    // use, so whichever path is measured first would otherwise be charged for
    // them — which is exactly what happened, and made streaming look worse than
    // collecting.
    for _ in expand_rules(&rules, &DATABASE, budget(10_000_000)) {}

    let mut streamed = 0u64;
    let streaming_peak = peak_bytes(|| {
        for _ in expand_rules(&rules, &DATABASE, budget(10_000_000)) {
            streamed += 1;
        }
    });

    let mut collected = 0usize;
    let collecting_peak = peak_bytes(|| {
        collected = expand_rules_to_vec(&rules, &DATABASE, budget(10_000_000)).len();
    });

    assert_eq!(streamed as usize, collected, "both paths, same numbers");
    assert!(
        streamed > 1_000,
        "expected a real expansion, got {streamed}"
    );
    assert!(
        collecting_peak >= collected * std::mem::size_of::<i64>(),
        "collecting {collected} numbers should pay for the list, saw {collecting_peak}"
    );
    assert!(
        streaming_peak * 4 < collecting_peak,
        "streaming peaked at {streaming_peak} against {collecting_peak} for collecting"
    );
}

#[test]
fn peak_memory_does_not_grow_with_the_size_of_the_output() {
    // One rule set ten times wider than the other. If anything on the streaming
    // path buffered, this would show up as a proportional jump.
    let narrow = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Pattern(Pattern::parse("390212345xxx").expect("pattern")),
    )]);
    let wide = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Pattern(Pattern::parse("39021234xxxx").expect("pattern")),
    )]);

    let mut narrow_count = 0u64;
    let narrow_peak = peak_bytes(|| {
        for _ in expand_rules(&narrow, &DATABASE, budget(10_000_000)) {
            narrow_count += 1;
        }
    });

    let mut wide_count = 0u64;
    let wide_peak = peak_bytes(|| {
        for _ in expand_rules(&wide, &DATABASE, budget(10_000_000)) {
            wide_count += 1;
        }
    });

    assert!(
        wide_count >= narrow_count * 5,
        "the wide set should be much bigger: {narrow_count} vs {wide_count}"
    );
    assert!(
        wide_peak <= narrow_peak * 2,
        "output grew {narrow_count}->{wide_count} but peak grew {narrow_peak}->{wide_peak}"
    );
}

#[test]
fn a_number_is_never_emitted_twice_even_when_two_rules_cover_it() {
    // Deduplication is a comparison against the last value emitted, which only
    // works because the merge is ordered. If that ordering broke, this catches
    // it without a set to compare against.
    let rules = wide_rules();

    // Takes the same lock as the measurements: this one allocates heavily, and
    // running alongside a measurement would show up as that measurement's peak.
    let mut previous: Option<i64> = None;
    peak_bytes(|| {
        for n in expand_rules(&rules, &DATABASE, budget(10_000_000)) {
            if let Some(p) = previous {
                assert!(p < n, "{p} then {n} is not strictly ascending");
            }
            previous = Some(n);
        }
    });
    assert!(previous.is_some());
}
