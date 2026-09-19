//! Isolated allocation measurements. Foreign sinks must also release batches;
//! this test measures the Rust side, not Swift ARC or JVM/JNA heap usage.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use callerfilter_core::ffi::*;
use callerfilter_core::number_metadata::NumberMetadata;

#[path = "../../tools/ffi-tests/fixtures.rs"]
mod fixtures;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
struct Counting;
// SAFETY: delegates every allocation/deallocation to System with the original
// layout. The accounting uses only atomics, never allocates or dereferences p.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live =
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed) + layout.size() as isize;
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn measure<T>(body: impl FnOnce() -> T) -> (T, usize, usize, std::time::Duration) {
    let base = LIVE.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    let start = Instant::now();
    let result = body();
    let elapsed = start.elapsed();
    let peak = (PEAK.load(Ordering::SeqCst) - base).max(0) as usize;
    let retained = (LIVE.load(Ordering::SeqCst) - base).max(0) as usize;
    (result, peak, retained, elapsed)
}

#[derive(Default)]
struct DiscardingSink {
    count: AtomicU64,
}
impl ExpansionSink for DiscardingSink {
    fn on_batch(&self, values: Vec<i64>) -> Result<ExpansionStatus, FfiError> {
        assert!(values.len() <= 32);
        self.count.fetch_add(values.len() as u64, Ordering::Relaxed);
        Ok(ExpansionStatus::Continue)
    }
}
fn rules(pattern: &str) -> Arc<PreparedRules> {
    PreparedRules::new(vec![RuleInput {
        id: 1,
        effect: EffectInput::Deny,
        matcher: MatcherInput::Pattern {
            value: pattern.into(),
        },
    }])
    .unwrap()
}
fn expand(
    rules: &PreparedRules,
    snapshot: &Snapshot,
    sink: Arc<dyn ExpansionSink>,
) -> ExpansionOutput {
    expand_rules_batched(
        rules,
        snapshot,
        BudgetInput {
            max_entries: 10_000,
            max_candidates: 10_000,
        },
        32,
        sink,
    )
    .unwrap()
}

#[test]
fn streaming_ffi_peak_does_not_scale_with_output() {
    let snapshot = Snapshot::new(fixtures::numbering(), vec![]).unwrap();
    let small = rules("390200000xxx");
    let large = rules("39020000xxxx");
    assert!(matches!(
        expand_rules_batched(
            &small,
            &snapshot,
            BudgetInput {
                max_entries: 10_000,
                max_candidates: 10_000
            },
            0,
            Arc::new(DiscardingSink::default())
        ),
        Err(FfiError::InvalidBatchSize)
    ));
    // Warm only the expansion caches. Runtime cold loading is measured separately.
    expand(&large, &snapshot, Arc::new(DiscardingSink::default()));
    let (small_result, small_peak, _, _) =
        measure(|| expand(&small, &snapshot, Arc::new(DiscardingSink::default())));
    let (large_result, large_peak, _, _) =
        measure(|| expand(&large, &snapshot, Arc::new(DiscardingSink::default())));
    assert!(matches!(
        small_result,
        ExpansionOutput::Fits { entries: 1_000 }
    ));
    assert!(matches!(
        large_result,
        ExpansionOutput::Fits { entries: 10_000 }
    ));
    assert!(
        large_peak <= small_peak * 2,
        "10x output: {small_peak} -> {large_peak} peak"
    );

    struct Collecting(std::sync::Mutex<Vec<i64>>);
    impl ExpansionSink for Collecting {
        fn on_batch(&self, values: Vec<i64>) -> Result<ExpansionStatus, FfiError> {
            self.0.lock().unwrap().extend(values);
            Ok(ExpansionStatus::Continue)
        }
    }
    let (_, collecting_peak, _, _) = measure(|| {
        let sink = Arc::new(Collecting(Default::default()));
        expand(&large, &snapshot, sink.clone());
        assert_eq!(sink.0.lock().unwrap().len(), 10_000);
    });
    assert!(
        large_peak * 4 < collecting_peak,
        "stream={large_peak}, collect={collecting_peak}"
    );
    println!("FFI memory: small_peak={small_peak}, large_peak={large_peak}, collecting_peak={collecting_peak} bytes");
}

#[test]
#[ignore = "isolated release runtime measurement; requires converter output, run alone"]
fn runtime_metadata_cold_and_coexisting_databases() {
    assert!(!cfg!(debug_assertions), "measure with --release");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../datasets/build/number-metadata.cfnd");
    let bytes = std::fs::read(path).expect("run the build-number-metadata converter first");
    // No XML, conversion, or bundled DATABASE initialization inside this process.
    let (old, cold_peak, old_live, cold_time) = measure(|| NumberMetadata::parse(&bytes).unwrap());
    let (new, replacement_peak, new_live, replacement_time) =
        measure(|| NumberMetadata::parse(&bytes).unwrap());
    assert_eq!(old.upstream(), new.upstream());
    println!("Runtime CFNM: payload={} bytes; cold_parse={cold_time:?}; cold_peak={cold_peak}; old_live={old_live}; second_parse={replacement_time:?}; new_live={new_live}; coexist_peak={} bytes", bytes.len(), old_live + replacement_peak);
    drop(old);
    assert!(
        !new.upstream().is_empty(),
        "new DB survives old snapshot release"
    );
}
