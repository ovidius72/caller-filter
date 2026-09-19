use callerfilter_core::dataset::{Builder, Kind};
use callerfilter_core::ffi::{
    evaluate_number, expand_rules_batched, normalize_number, BudgetInput, EffectInput,
    ExpansionOutput, ExpansionSink, ExpansionStatus, MatcherInput, PreparedRules, RuleInput,
    Snapshot,
};
use callerfilter_core::number_metadata::NumberMetadata;
use phonenumber::metadata::loader;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

#[path = "../../tools/ffi-tests/fixtures.rs"]
mod fixtures;

fn snapshot() -> Arc<Snapshot> {
    let numbering = NumberMetadata::build("test", Vec::new()).expect("numbering");
    let place = Builder::new()
        .build(Kind::Places, "en", "test")
        .expect("place");
    Snapshot::new(numbering, vec![place]).expect("snapshot")
}

fn real_snapshot() -> Arc<Snapshot> {
    let metadata = loader::load(Cursor::new(include_str!(
        "../../data/upstream/PhoneNumberMetadata.xml"
    )))
    .expect("metadata");
    let numbering = NumberMetadata::build("test", metadata).expect("numbering");
    Snapshot::new(numbering, Vec::new()).expect("snapshot")
}

#[derive(Default)]
struct Sink {
    values: Mutex<Vec<i64>>,
    cancel: bool,
}

impl ExpansionSink for Sink {
    fn on_batch(
        &self,
        values: Vec<i64>,
    ) -> Result<ExpansionStatus, callerfilter_core::ffi::FfiError> {
        self.values.lock().unwrap().extend(values);
        if self.cancel {
            Ok(ExpansionStatus::Cancel)
        } else {
            Ok(ExpansionStatus::Continue)
        }
    }
}

#[test]
#[ignore = "writes host binding fixtures into target/ffi-tests only"]
fn write_host_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/ffi-tests/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = fixtures::numbering();
    std::fs::write(dir.join("numbering.cfnd"), &bytes).unwrap();
    std::fs::write(
        dir.join("empty.cfnd"),
        NumberMetadata::build("empty", vec![]).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("places-en.cfds"),
        fixtures::places("en", &["3902", "3903"]),
    )
    .unwrap();
    std::fs::write(
        dir.join("places-user.cfds"),
        fixtures::places("test-language", &["3904"]),
    )
    .unwrap();
    std::fs::write(
        dir.join("places-new.cfds"),
        fixtures::places("en", &["3905"]),
    )
    .unwrap();
}

fn exact(id: u64, effect: EffectInput, digits: &str) -> RuleInput {
    RuleInput {
        id,
        effect,
        matcher: MatcherInput::Exact {
            digits: digits.into(),
        },
    }
}

#[test]
fn snapshot_identity_is_checked_for_both_explain_and_expansion() {
    use callerfilter_core::ffi::{explain_rule, prepare_rules_for_snapshot, FfiError};
    let a = Snapshot::new(
        fixtures::numbering(),
        vec![fixtures::places("en", &["3902", "3903"])],
    )
    .unwrap();
    let b = Snapshot::new(
        fixtures::numbering(),
        vec![fixtures::places("en", &["3904"])],
    )
    .unwrap();
    let rules = prepare_rules_for_snapshot(
        vec![RuleInput {
            id: 1,
            effect: EffectInput::Deny,
            matcher: MatcherInput::Location {
                name: "Test Town".into(),
                language: "en".into(),
            },
        }],
        &a,
    )
    .unwrap();
    assert_eq!(
        a.place_prefixes("Test Town".into(), "en".into()),
        ["3902", "3903"]
    );
    assert!(matches!(
        explain_rule(1, &rules, &b, vec![]),
        Err(FfiError::SnapshotMismatch)
    ));
    assert!(matches!(
        expand_rules_batched(
            &rules,
            &b,
            BudgetInput {
                max_entries: 1,
                max_candidates: 1
            },
            1,
            Arc::new(Sink::default())
        ),
        Err(FfiError::SnapshotMismatch)
    ));
    assert!(explain_rule(1, &rules, &a, vec![]).is_ok());
    drop(a);
    assert!(matches!(
        evaluate_number("+390200000000".into(), None, &rules)
            .unwrap()
            .decision,
        callerfilter_core::ffi::DecisionOutput::Block
    ));
}

#[test]
fn direct_rules_remain_portable_even_when_prepared_with_a_snapshot() {
    use callerfilter_core::ffi::{explain_rule, prepare_rules_for_snapshot};
    let a = snapshot();
    let b = snapshot();
    let rules = prepare_rules_for_snapshot(vec![exact(1, EffectInput::Deny, "123")], &a).unwrap();
    assert!(explain_rule(1, &rules, &b, vec![]).is_ok());
}

#[test]
fn conflicts_expose_both_ids_and_duplicate_ids_are_rejected() {
    use callerfilter_core::ffi::FfiError;
    let rules = PreparedRules::new(vec![
        exact(10, EffectInput::Deny, "123"),
        exact(20, EffectInput::Allow, "123"),
    ])
    .unwrap();
    let conflicts = rules.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!((conflicts[0].first, conflicts[0].second), (10, 20));
    assert!(
        evaluate_number("123".into(), None, &rules)
            .unwrap()
            .contested
    );
    assert!(matches!(
        PreparedRules::new(vec![
            exact(10, EffectInput::Deny, "123"),
            exact(10, EffectInput::Allow, "123")
        ]),
        Err(FfiError::DuplicateRuleId { id: 10 })
    ));
}

#[test]
fn international_normalization_ignores_absent_and_invalid_hints() {
    let snapshot = Snapshot::new(fixtures::numbering(), vec![]).unwrap();
    for hint in [None, Some("DE"), Some("invalid")] {
        assert_eq!(
            normalize_number("+39 0200000000".into(), hint.map(str::to_owned), &snapshot)
                .unwrap()
                .e164,
            "+390200000000"
        );
    }
}

#[test]
fn batching_handles_cancellation_errors_and_huge_requested_capacity() {
    use callerfilter_core::ffi::FfiError;
    let snapshot = Snapshot::new(fixtures::numbering(), vec![]).unwrap();
    let rules = PreparedRules::new(vec![
        exact(1, EffectInput::Deny, "390200000000"),
        exact(2, EffectInput::Deny, "390200000001"),
    ])
    .unwrap();
    let sink = Arc::new(Sink {
        cancel: true,
        ..Default::default()
    });
    let output = expand_rules_batched(
        &rules,
        &snapshot,
        BudgetInput {
            max_entries: 2,
            max_candidates: 2,
        },
        1,
        sink.clone(),
    )
    .unwrap();
    assert!(matches!(output, ExpansionOutput::Cancelled { entries: 1 }));
    assert_eq!(sink.values.lock().unwrap().len(), 1);
    struct Failing;
    impl ExpansionSink for Failing {
        fn on_batch(&self, _: Vec<i64>) -> Result<ExpansionStatus, FfiError> {
            Err(FfiError::Callback {
                reason: "stop".into(),
            })
        }
    }
    assert!(
        matches!(expand_rules_batched(&rules, &snapshot, BudgetInput {max_entries: 2, max_candidates: 2}, 1, Arc::new(Failing)), Err(FfiError::Callback {reason}) if reason == "stop")
    );
    let sink = Arc::new(Sink::default());
    assert!(matches!(
        expand_rules_batched(
            &rules,
            &snapshot,
            BudgetInput {
                max_entries: 2,
                max_candidates: 2
            },
            u32::MAX,
            sink.clone()
        )
        .unwrap(),
        ExpansionOutput::Fits { entries: 2 }
    ));
    assert_eq!(sink.values.lock().unwrap().len(), 2);
}

#[test]
fn ffi_snapshot_and_prepared_rules_are_owned_handles() {
    let snapshot = snapshot();
    assert_eq!(snapshot.numbering_upstream(), "test");
    assert_eq!(snapshot.dataset_versions().len(), 1);
    let rules = PreparedRules::new(vec![exact(1, EffectInput::Deny, "123")]).expect("rules");
    assert_eq!(rules.len(), 1);
}

#[test]
fn failed_snapshot_replacement_does_not_publish_partial_data() {
    let numbering = NumberMetadata::build("test", Vec::new()).expect("numbering");
    let valid = Builder::new()
        .build(Kind::Places, "en", "test")
        .expect("place");
    assert!(Snapshot::new(numbering, vec![valid, b"broken".to_vec()]).is_err());
}

#[test]
fn ffi_rejects_invalid_number_and_rule_inputs() {
    let snapshot = snapshot();
    assert!(normalize_number("not a number".into(), None, &snapshot).is_err());
    assert!(PreparedRules::new(vec![RuleInput {
        id: 1,
        effect: EffectInput::Deny,
        matcher: MatcherInput::Pattern {
            value: "xxx".into()
        },
    }])
    .is_err());
}

#[test]
fn normalization_with_region_absent_from_snapshot_does_not_panic() {
    assert!(normalize_number("0212345678".into(), Some("IT".into()), &snapshot()).is_err());
}

#[test]
fn ffi_evaluation_preserves_allow_default() {
    let snapshot = snapshot();
    let rules = PreparedRules::new(Vec::new()).expect("rules");
    let verdict = evaluate_number("123".into(), None, &rules).expect("evaluation");
    assert!(matches!(
        verdict.decision,
        callerfilter_core::ffi::DecisionOutput::Allow
    ));
    drop(snapshot);
}

#[test]
fn merged_union_is_capped_before_first_callback() {
    let snapshot = real_snapshot();
    let rules = PreparedRules::new(vec![
        exact(1, EffectInput::Deny, "390212345678"),
        exact(2, EffectInput::Deny, "390212345679"),
    ])
    .expect("rules");
    let sink = Arc::new(Sink::default());
    let result = expand_rules_batched(
        &rules,
        &snapshot,
        BudgetInput {
            max_entries: 1,
            max_candidates: 10,
        },
        2,
        sink.clone(),
    )
    .expect("expansion");
    assert!(matches!(
        result,
        ExpansionOutput::TooBroad {
            upper_bound: 2,
            exact: true,
            ..
        }
    ));
    assert!(sink.values.lock().unwrap().is_empty());
}

#[test]
fn overlap_and_allow_subtraction_are_counted_after_merge() {
    let snapshot = real_snapshot();
    let overlap = PreparedRules::new(vec![
        exact(1, EffectInput::Deny, "390212345678"),
        exact(2, EffectInput::Deny, "390212345678"),
    ])
    .expect("rules");
    let sink = Arc::new(Sink::default());
    let result = expand_rules_batched(
        &overlap,
        &snapshot,
        BudgetInput {
            max_entries: 1,
            max_candidates: 10,
        },
        2,
        sink.clone(),
    )
    .expect("expansion");
    assert!(matches!(result, ExpansionOutput::Fits { entries: 1 }));
    assert_eq!(sink.values.lock().unwrap().as_slice(), &[390212345678]);

    let carved = PreparedRules::new(vec![
        RuleInput {
            id: 1,
            effect: EffectInput::Deny,
            matcher: MatcherInput::Pattern {
                value: "39021234567x".into(),
            },
        },
        exact(2, EffectInput::Allow, "390212345678"),
    ])
    .expect("rules");
    let eval = evaluate_number("+390212345678".into(), None, &carved).expect("evaluation");
    assert!(matches!(
        eval.decision,
        callerfilter_core::ffi::DecisionOutput::Allow
    ));
    let sink = Arc::new(Sink::default());
    let result = expand_rules_batched(
        &carved,
        &snapshot,
        BudgetInput {
            max_entries: 9,
            max_candidates: 10,
        },
        2,
        sink.clone(),
    )
    .expect("expansion");
    assert!(
        matches!(result, ExpansionOutput::Fits { entries: 9 }),
        "got {result:?}"
    );
    let emitted = sink.values.lock().unwrap();
    assert_eq!(emitted.len(), 9);
    assert!(!emitted.contains(&390212345678));
}

#[test]
fn non_expandable_rule_is_reported_before_valid_stream() {
    let snapshot = real_snapshot();
    let rules = PreparedRules::new(vec![
        exact(1, EffectInput::Deny, "390212345678"),
        RuleInput {
            id: 2,
            effect: EffectInput::Deny,
            matcher: MatcherInput::Suffix {
                digits: "678".into(),
            },
        },
    ])
    .expect("rules");
    let sink = Arc::new(Sink::default());
    let result = expand_rules_batched(
        &rules,
        &snapshot,
        BudgetInput {
            max_entries: 10,
            max_candidates: 10,
        },
        2,
        sink.clone(),
    )
    .expect("expansion");
    assert!(matches!(result, ExpansionOutput::NotExpandable { .. }));
    assert!(sink.values.lock().unwrap().is_empty());
}
