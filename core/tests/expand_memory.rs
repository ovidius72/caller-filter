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

/// A rule set whose expansion is far larger than any sensible buffer.
fn wide_rules() -> RuleSet {
    RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("390212xxxxxx").expect("pattern")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("3902123xxxxx").expect("pattern")),
        ),
    ])
}

#[test]
fn streaming_a_large_expansion_holds_almost_nothing() {
    let rules = wide_rules();
    let mut count = 0u64;

    let peak = peak_bytes(|| {
        for _ in expand_rules(&rules, &DATABASE, budget(10_000_000)) {
            count += 1;
        }
    });

    assert!(count > 100_000, "expected a big expansion, got {count}");
    // The claim is that memory is not proportional to the list, so the test
    // says exactly that rather than picking a byte count out of the air: less
    // than one byte held per number emitted. A Vec of i64 costs eight, and the
    // test below shows it paying them. What is actually live here is the merge
    // state, two small buffers, and the metadata regex caches warming up.
    assert!(
        (peak as u64) < count,
        "streaming {count} numbers peaked at {peak} bytes, which is proportional to the output"
    );
}

#[test]
fn collecting_the_same_expansion_costs_memory_proportional_to_it() {
    // The contrast is the point. This is what the extension must not do, and
    // what expand_rules used to do unconditionally.
    let rules = wide_rules();
    let mut count = 0usize;

    let peak = peak_bytes(|| {
        count = expand_rules_to_vec(&rules, &DATABASE, budget(10_000_000)).len();
    });

    assert!(count > 100_000);
    assert!(
        peak >= count * std::mem::size_of::<i64>(),
        "collecting {count} numbers should cost at least the list itself, saw {peak}"
    );
}

#[test]
fn peak_memory_does_not_grow_with_the_size_of_the_output() {
    // One rule set ten times wider than the other. If anything on the streaming
    // path buffered, this would show up as a proportional jump.
    let narrow = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Pattern(Pattern::parse("39021234xxxx").expect("pattern")),
    )]);
    let wide = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Pattern(Pattern::parse("3902123xxxxx").expect("pattern")),
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
