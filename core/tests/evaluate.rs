//! Evaluator tests.
//!
//! Outside `core/src` for the same reason as the rule model tests: the
//! Guidelines §1 lint refuses digit runs in the shipped crate, and an
//! integration test binary ships with nothing.
//!
//! This is the heaviest coverage in the project on purpose. Every platform
//! calls this one function, so a bug here is a bug everywhere at once.

use std::time::Instant;

use callerfilter_core::{
    evaluate, Call, Decision, Digits, Effect, Matcher, Pattern, Rule, RuleId, RuleSet, E164,
};

fn digits(s: &str) -> Digits {
    Digits::parse(s).expect("valid digits")
}

fn pattern(s: &str) -> Pattern {
    Pattern::parse(s).expect("valid pattern")
}

fn rule(id: u64, effect: Effect, matcher: Matcher) -> Rule {
    Rule::new(RuleId(id), effect, matcher)
}

fn decide(number: &str, rules: &RuleSet) -> Decision {
    let n = E164::new(number).expect("a normalised number");
    evaluate(&Call::new(n), rules).decision
}

// ------------------------------------------------------------- nothing to do

#[test]
fn a_call_nothing_matches_is_allowed() {
    let rules = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::StartsWith(digits("0987")),
    )]);

    assert_eq!(decide("0512345678", &rules), Decision::Allow);
}

#[test]
fn an_empty_rule_set_allows_everything() {
    assert_eq!(decide("0987777212", &RuleSet::default()), Decision::Allow);
}

// ------------------------------------------------------------ most specific

#[test]
fn a_deny_on_a_range_blocks_a_number_inside_it() {
    let rules = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::StartsWith(digits("0987777")),
    )]);

    assert_eq!(decide("0987777212", &rules), Decision::Block);
}

#[test]
fn an_allow_on_an_exact_number_beats_a_deny_on_its_range() {
    // R2's headline case. Neither rule is ordered by hand; the longer wins.
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);

    assert_eq!(decide("0987777212", &rules), Decision::Allow);
    assert_eq!(
        decide("0987777999", &rules),
        Decision::Block,
        "the exception is one number wide"
    );
}

#[test]
fn authoring_order_does_not_change_the_answer() {
    let forwards = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);
    let backwards = RuleSet::new(vec![
        rule(1, Effect::Allow, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
    ]);

    assert_eq!(decide("0987777212", &forwards), Decision::Allow);
    assert_eq!(decide("0987777212", &backwards), Decision::Allow);
}

#[test]
fn three_levels_of_nesting_resolve_to_the_narrowest() {
    // Deny a wide range, allow a narrower one inside it, deny one number back.
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987"))),
        rule(2, Effect::Allow, Matcher::StartsWith(digits("0987777"))),
        rule(3, Effect::Deny, Matcher::Exact(digits("0987777212"))),
    ]);

    assert_eq!(decide("0987123456", &rules), Decision::Block);
    assert_eq!(decide("0987777999", &rules), Decision::Allow);
    assert_eq!(decide("0987777212", &rules), Decision::Block);
}

#[test]
fn a_rule_matching_every_number_still_loses_to_anything_narrower() {
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Pattern(pattern("0xxxxxxxxx"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);

    assert_eq!(decide("0987777212", &rules), Decision::Allow);
    assert_eq!(decide("0512345678", &rules), Decision::Block);
}

#[test]
fn an_allow_with_no_enclosing_deny_changes_nothing() {
    // Legal, and a no-op: absence from any deny already means allowed. It must
    // not be refused, and it must not start blocking anything else.
    let rules = RuleSet::new(vec![rule(
        1,
        Effect::Allow,
        Matcher::Exact(digits("0987777212")),
    )]);

    assert_eq!(decide("0987777212", &rules), Decision::Allow);
    assert_eq!(decide("0512345678", &rules), Decision::Allow);
}

// ----------------------------------------------------------------- matchers

#[test]
fn each_matcher_decides_on_its_own_terms() {
    let exact = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::Exact(digits("0987777212")),
    )]);
    let ends = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::EndsWith(digits("212")),
    )]);
    let pat = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::Pattern(pattern("0987777xxx")),
    )]);

    assert_eq!(decide("0987777212", &exact), Decision::Block);
    assert_eq!(decide("098777721", &exact), Decision::Allow);

    assert_eq!(decide("0512345212", &ends), Decision::Block);
    assert_eq!(decide("0512345999", &ends), Decision::Allow);

    assert_eq!(decide("0987777212", &pat), Decision::Block);
    assert_eq!(
        decide("09877772123", &pat),
        Decision::Allow,
        "a pattern fixes the length"
    );
}

#[test]
fn a_caller_id_rule_matches_the_name_and_ignores_case() {
    let rules = RuleSet::new(vec![rule(
        1,
        Effect::Deny,
        Matcher::CallerId("Acme Ltd".into()),
    )]);
    let number = E164::new("0512345678").unwrap();

    let named = Call::new(number).with_caller_id("acme ltd");
    let other = Call::new(number).with_caller_id("Someone Else");
    let anonymous = Call::new(number);

    assert_eq!(evaluate(&named, &rules).decision, Decision::Block);
    assert_eq!(evaluate(&other, &rules).decision, Decision::Allow);
    assert_eq!(
        evaluate(&anonymous, &rules).decision,
        Decision::Allow,
        "no name offered, so a name rule cannot match"
    );
}

#[test]
fn a_number_rule_outranks_a_caller_id_rule_because_it_pins_digits() {
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::CallerId("Acme Ltd".into())),
        rule(2, Effect::Allow, Matcher::StartsWith(digits("05"))),
    ]);
    let number = E164::new("0512345678").unwrap();

    let call = Call::new(number).with_caller_id("Acme Ltd");
    assert_eq!(evaluate(&call, &rules).decision, Decision::Allow);
}

// ------------------------------------------------------------------- ties

#[test]
fn a_tie_between_opposing_rules_allows_and_is_flagged() {
    // Nothing picks a side here. The call is allowed because that is the
    // recoverable half — a blocked wanted call is silent, and on iOS the system
    // never even tells the app it happened. The user is asked at edit time.
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);
    let number = E164::new("0987777212").unwrap();

    let verdict = evaluate(&Call::new(number), &rules);

    assert_eq!(verdict.decision, Decision::Allow);
    assert!(verdict.contested, "the caller must be able to see it tied");
    assert_eq!(
        rules.conflicts().len(),
        1,
        "and the same pair is reportable off the call path"
    );
}

#[test]
fn a_tie_between_agreeing_rules_is_not_contested() {
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Deny, Matcher::Exact(digits("0987777212"))),
    ]);
    let number = E164::new("0987777212").unwrap();

    let verdict = evaluate(&Call::new(number), &rules);

    assert_eq!(verdict.decision, Decision::Block);
    assert!(
        !verdict.contested,
        "duplicates agree, so there is no tie to ask about"
    );
}

#[test]
fn a_prefix_and_a_suffix_of_equal_width_tie_when_both_match() {
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987"))),
        rule(2, Effect::Allow, Matcher::EndsWith(digits("7212"))),
    ]);
    let number = E164::new("0987777212").unwrap();

    let verdict = evaluate(&Call::new(number), &rules);
    assert!(verdict.contested);

    // The same pair does not tie for a number only one of them matches.
    let only_prefix = E164::new("0987123456").unwrap();
    let verdict = evaluate(&Call::new(only_prefix), &rules);
    assert!(!verdict.contested);
    assert_eq!(verdict.decision, Decision::Block);
}

// -------------------------------------------------------------- diagnostics

#[test]
fn the_verdict_names_the_rule_that_decided() {
    // R2 requires a rule never silently start or stop applying, so the caller
    // has to be able to say which one acted.
    let rules = RuleSet::new(vec![
        rule(7, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
        rule(9, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);
    let number = E164::new("0987777212").unwrap();

    let verdict = evaluate(&Call::new(number), &rules);
    assert_eq!(verdict.matched.map(|r| r.id.0), Some(9));
}

// ---------------------------------------------------------------- behaviour
// under a rule count no user will ever reach

#[test]
fn a_large_rule_set_stays_fast() {
    // A tripwire, not a benchmark. Real rule sets are tens of rules — it is the
    // iOS *expansion* that gets large, not the rule count — so this is far past
    // anything a person writes. It exists to make an accidental change from
    // "walk until we drop below the best match" to something quadratic show up
    // as a failure rather than as a phone that rings late.
    //
    // The Android screening callback must answer within five seconds or the
    // phone does not ring, so the real budget is enormous compared to this.
    // The bound below is deliberately loose to stay honest on a slow CI runner.
    let mut rules = Vec::new();
    for i in 0..10_000u64 {
        rules.push(rule(
            i,
            Effect::Deny,
            Matcher::Exact(digits(&format!("05{i:08}"))),
        ));
    }
    // One very specific allow, so the walk finds a winner early and stops.
    rules.push(rule(
        999_999,
        Effect::Allow,
        Matcher::Exact(digits("0987777212")),
    ));
    let rules = RuleSet::new(rules);

    let hit = E164::new("0987777212").unwrap();
    let miss = E164::new("0111111111").unwrap();

    let started = Instant::now();
    for _ in 0..1_000 {
        assert_eq!(evaluate(&Call::new(hit), &rules).decision, Decision::Allow);
        assert_eq!(evaluate(&Call::new(miss), &rules).decision, Decision::Allow);
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "2,000 evaluations over 10,001 rules took {elapsed:?}"
    );
}

#[test]
fn conflict_reporting_is_not_needed_to_get_an_answer() {
    // conflicts() costs a pass over the rules, so the call path must never need
    // it. This is the same set as the tie test, evaluated without asking.
    let rules = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);
    let number = E164::new("0987777212").unwrap();

    let verdict = evaluate(&Call::new(number), &rules);
    assert!(verdict.contested);
    assert_eq!(verdict.decision, Decision::Allow);
}
