//! What a rule does on each platform.
//!
//! Outside `core/src` because the §1 lint refuses digit runs in the shipped
//! crate.
//!
//! The surfaces below are fixtures describing the platforms verified in F001.
//! They are values, not code: adding one is describing it, not editing the
//! core.

use callerfilter_core::{
    explain, Budget, Digits, Effect, EntryLimit, Matcher, NotExpandable, Pattern,
    PlatformVerdict as Verdict, Prefixes, Rule, RuleId, RuleSet, Surface,
};
use phonenumber::metadata::DATABASE;

/// Android calls: every rule runs as the call arrives, caller name included.
fn android_calls() -> Surface {
    Surface {
        id: "android-calls".into(),
        evaluates_live: true,
        matches_caller_id: true,
        budget: None,
    }
}

/// iOS messages: live too, and it can see the sender.
fn ios_messages() -> Surface {
    Surface {
        id: "ios-messages".into(),
        evaluates_live: true,
        matches_caller_id: true,
        budget: None,
    }
}

/// iOS calls: nothing runs when the phone rings, so the numbers are listed in
/// advance, and there is no name to match on.
fn ios_calls(entries: u64) -> Surface {
    Surface {
        id: "ios-calls".into(),
        evaluates_live: false,
        matches_caller_id: false,
        budget: Some(Budget {
            max_entries: entries,
            max_candidates: entries.saturating_mul(10),
        }),
    }
}

fn all(entries: u64) -> Vec<Surface> {
    vec![android_calls(), ios_messages(), ios_calls(entries)]
}

fn digits(s: &str) -> Digits {
    Digits::parse(s).expect("digits")
}

/// One prefix, as the set of one that a prefix rule now holds.
fn prefix(s: &str) -> Prefixes {
    Prefixes::one(digits(s))
}

fn deny(id: u64, m: Matcher) -> Rule {
    Rule::new(RuleId(id), Effect::Deny, m)
}

#[test]
fn a_narrow_rule_runs_live_on_one_platform_and_is_listed_on_another() {
    let rule = deny(1, Matcher::Exact(digits("390212345678")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(10_000));

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    assert_eq!(e.on("ios-messages"), Some(&Verdict::AppliesLive));
    assert_eq!(e.on("ios-calls"), Some(&Verdict::Fits { entries: 1 }));
}

#[test]
fn a_rule_too_wide_for_the_phone_still_runs_live_elsewhere() {
    // This is the asymmetry the user has to be told about: the same rule works
    // on one phone and not the other, and the figure shown is real.
    let rule = deny(1, Matcher::StartsWith(prefix("3902")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(1_000));

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    match e.on("ios-calls") {
        Some(Verdict::TooBroad { upper_bound }) => assert!(*upper_bound > 1_000),
        other => panic!("expected too broad, got {other:?}"),
    }
}

#[test]
fn a_caller_id_rule_runs_live_but_cannot_be_listed() {
    // iOS calls carry no name. The rule is not broken and not too big — it
    // simply cannot apply there, which is a different thing to say.
    let rule = deny(1, Matcher::CallerId("Acme Ltd".into()));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(10_000));

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    assert_eq!(e.on("ios-messages"), Some(&Verdict::AppliesLive));
    assert_eq!(
        e.on("ios-calls"),
        Some(&Verdict::Inexpressible(NotExpandable::CallerId))
    );
}

#[test]
fn a_suffix_rule_runs_live_but_cannot_be_listed() {
    let rule = deny(1, Matcher::EndsWith(digits("7212")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(10_000));

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    assert_eq!(
        e.on("ios-calls"),
        Some(&Verdict::Inexpressible(NotExpandable::Suffix))
    );
}

#[test]
fn an_allow_that_no_deny_covers_does_nothing_anywhere() {
    // Not being blocked is already the default, so this changes nothing on any
    // platform. The UI must not imply it is protecting anything.
    let allow = Rule::new(
        RuleId(1),
        Effect::Allow,
        Matcher::Exact(digits("390212345678")),
    );
    let rules = RuleSet::new(vec![allow.clone()]);

    let e = explain(&allow, &rules, &DATABASE, &all(10_000));

    for surface in ["android-calls", "ios-messages", "ios-calls"] {
        assert_eq!(e.on(surface), Some(&Verdict::NoEffect), "on {surface}");
    }
}

#[test]
fn an_allow_inside_a_deny_does_something_everywhere() {
    let allow = Rule::new(
        RuleId(2),
        Effect::Allow,
        Matcher::Exact(digits("390212345678")),
    );
    let rules = RuleSet::new(vec![
        deny(1, Matcher::StartsWith(prefix("39021234567"))),
        allow.clone(),
    ]);

    let e = explain(&allow, &rules, &DATABASE, &all(10_000));

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    assert_ne!(e.on("ios-calls"), Some(&Verdict::NoEffect));
}

#[test]
fn a_rule_for_numbers_that_do_not_exist_reports_no_effect_where_it_is_listed() {
    // It expands cleanly, to nothing. The rule is real, will never fire, and
    // the user would otherwise never find out.
    let rule = deny(1, Matcher::Exact(digits("391112345678")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(10_000));

    assert_eq!(e.on("ios-calls"), Some(&Verdict::NoEffect));
    assert_eq!(
        e.on("android-calls"),
        Some(&Verdict::AppliesLive),
        "a live surface cannot know in advance that nothing will match"
    );
}

#[test]
fn digits_belonging_to_no_country_cannot_be_listed() {
    let rule = deny(1, Matcher::StartsWith(prefix("9999999")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &all(10_000));

    assert_eq!(
        e.on("ios-calls"),
        Some(&Verdict::Inexpressible(NotExpandable::UnknownCountry))
    );
}

// ------------------------------------------------------------- the budget

#[test]
fn the_budget_decides_the_verdict_and_it_is_injected() {
    // Same rule, same data, two measured limits, two answers. Nothing about the
    // limit is written into the core.
    let rule = deny(
        1,
        Matcher::Pattern(Pattern::parse("39021234xxxx").expect("p")),
    );
    let rules = RuleSet::new(vec![rule.clone()]);

    let generous = explain(&rule, &rules, &DATABASE, &all(100_000));
    let mean = explain(&rule, &rules, &DATABASE, &all(10));

    assert!(matches!(
        generous.on("ios-calls"),
        Some(Verdict::Fits { .. })
    ));
    assert!(matches!(
        mean.on("ios-calls"),
        Some(Verdict::TooBroad { .. })
    ));
}

#[test]
fn an_expansion_exactly_at_the_limit_still_fits() {
    // Off-by-one here means telling a user their rule is too big when it is not.
    let rule = deny(
        1,
        Matcher::Pattern(Pattern::parse("39021234567x").expect("p")),
    );
    let rules = RuleSet::new(vec![rule.clone()]);

    let entries = match explain(&rule, &rules, &DATABASE, &all(1_000_000)).on("ios-calls") {
        Some(Verdict::Fits { entries }) => *entries,
        other => panic!("expected it to fit, got {other:?}"),
    };

    let exact = explain(&rule, &rules, &DATABASE, &all(entries));
    assert_eq!(
        exact.on("ios-calls"),
        Some(&Verdict::Fits { entries }),
        "a rule that is exactly the size of the budget fits in it"
    );
}

// -------------------------------------------------------- adding a surface

#[test]
fn a_new_platform_is_described_rather_than_coded() {
    // The point of surfaces being data. This one is invented here, in a test,
    // with no change to the core — which is what F002 needs for HarmonyOS.
    let invented = Surface {
        id: "some-future-phone".into(),
        evaluates_live: false,
        matches_caller_id: false,
        budget: Some(Budget::from_entry_limit(EntryLimit(500), 10)),
    };
    let rule = deny(1, Matcher::Exact(digits("390212345678")));
    let rules = RuleSet::new(vec![rule.clone()]);

    let e = explain(&rule, &rules, &DATABASE, &[invented]);

    assert_eq!(
        e.on("some-future-phone"),
        Some(&Verdict::Fits { entries: 1 })
    );
}
