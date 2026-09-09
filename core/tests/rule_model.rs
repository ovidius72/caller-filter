//! Rule model tests that need number-shaped fixtures.
//!
//! These live outside `core/src` on purpose. The Guidelines §1 lint refuses
//! digit runs anywhere under `core/src`, and that strictness is worth keeping:
//! a "just this once" number in the crate is how hardcoded numbering starts.
//! An integration test compiles to its own binary and ships with nothing, so
//! the fixtures here are not numbering data compiled into the core.
//!
//! The numbers below are shapes, not real subscribers, and no test depends on
//! any of them belonging to a particular country.

use callerfilter_core::{
    Authored, Digits, Effect, LocationRef, Matcher, Pattern, PrefixSource, Rule, RuleId, RuleSet,
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

/// A geocoder stub. The real one arrives in P004(F002).
struct FixedPlace(Option<&'static str>);

impl PrefixSource for FixedPlace {
    fn prefix_for(&self, _location: &LocationRef) -> Option<Digits> {
        self.0.map(digits)
    }
}

// ---------------------------------------------------------------- resolution

#[test]
fn a_prefix_and_a_location_both_become_leading_digits() {
    let places = FixedPlace(Some("0212"));

    let from_prefix = Authored::Prefix(digits("0212")).resolve(&places).unwrap();
    let from_place = Authored::Location(LocationRef("somewhere".into()))
        .resolve(&places)
        .unwrap();

    assert_eq!(from_prefix, Matcher::StartsWith(digits("0212")));
    assert_eq!(
        from_prefix, from_place,
        "one implementation of leading-digit matching, three ways to author it"
    );
}

#[test]
fn a_location_with_no_prefix_in_the_data_does_not_resolve() {
    // Metadata changes. A place that resolved last month may not today, and the
    // user has to be told the rule stopped matching rather than left guessing.
    let resolved = Authored::Location(LocationRef("nowhere".into())).resolve(&FixedPlace(None));
    assert_eq!(resolved, None);
}

// --------------------------------------------------------------- specificity

#[test]
fn an_allow_on_an_exact_number_outranks_a_deny_on_its_range() {
    // The headline case from R2: no manual ordering, the longer rule wins.
    let allow = rule(1, Effect::Allow, Matcher::Exact(digits("0987777212")));
    let deny = rule(2, Effect::Deny, Matcher::StartsWith(digits("0987777")));

    assert!(allow.specificity() > deny.specificity());
}

#[test]
fn nested_denies_of_different_width_rank_in_order() {
    let broad = rule(1, Effect::Deny, Matcher::StartsWith(digits("09")));
    let mid = rule(2, Effect::Deny, Matcher::StartsWith(digits("0987")));
    let narrow = rule(3, Effect::Deny, Matcher::StartsWith(digits("0987777")));

    assert!(narrow.specificity() > mid.specificity());
    assert!(mid.specificity() > broad.specificity());
}

#[test]
fn a_pattern_ranks_on_the_digits_it_pins_not_its_length() {
    let three_pinned = Matcher::Pattern(pattern("098xxxxxxx")).specificity();
    let seven_pinned = Matcher::Pattern(pattern("0987777xxx")).specificity();

    assert!(seven_pinned > three_pinned);
    assert_eq!(three_pinned.pinned(), 3);
    assert!(
        three_pinned.fixes_length(),
        "a pattern has one atom per digit"
    );
}

#[test]
fn a_rule_set_is_ordered_from_most_specific_to_least() {
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("09"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
        rule(3, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
    ]);

    let order: Vec<u64> = set.iter().map(|r| r.id.0).collect();
    assert_eq!(order, vec![2, 3, 1]);
}

#[test]
fn a_rule_matching_every_number_sorts_last_but_is_still_legal() {
    let catch_all = rule(1, Effect::Deny, Matcher::Pattern(pattern("0xxxxxxxxx")));
    let specific = rule(2, Effect::Deny, Matcher::Exact(digits("0987777212")));

    let set = RuleSet::new(vec![catch_all, specific]);
    let order: Vec<u64> = set.iter().map(|r| r.id.0).collect();
    assert_eq!(order, vec![2, 1]);
}

#[test]
fn an_allow_with_no_enclosing_deny_is_accepted() {
    // A no-op for iOS call expansion, since absence from the block list already
    // means allowed. The model must not forbid it; the UI explains it.
    let set = RuleSet::new(vec![rule(
        1,
        Effect::Allow,
        Matcher::Exact(digits("0987777212")),
    )]);

    assert_eq!(set.len(), 1);
    assert!(set.conflicts().is_empty());
}

// ------------------------------------------------------------------ conflict

#[test]
fn two_identical_rules_that_disagree_are_reported() {
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);

    let conflicts = set.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!((conflicts[0].a.0, conflicts[0].b.0), (1, 2));
}

#[test]
fn a_genuine_specificity_difference_is_not_a_conflict() {
    // This is the case that must NOT fire: the allow simply wins.
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987777"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777212"))),
    ]);

    assert!(set.conflicts().is_empty());
}

#[test]
fn equally_specific_rules_that_cannot_both_match_are_not_a_conflict() {
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Allow, Matcher::Exact(digits("0987777999"))),
    ]);

    assert!(
        set.conflicts().is_empty(),
        "different numbers never collide"
    );
}

#[test]
fn a_prefix_and_a_suffix_of_equal_width_are_reported_as_a_conflict() {
    // They tie, and ruling out an overlap would need to know how long numbers
    // are in this country — which the core is not allowed to know. Reporting is
    // the honest answer: R2 forbids resolving a tie silently, and a conflict we
    // failed to notice would be exactly that.
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::StartsWith(digits("0987"))),
        rule(2, Effect::Allow, Matcher::EndsWith(digits("7212"))),
    ]);

    assert_eq!(set.conflicts().len(), 1);
}

#[test]
fn two_patterns_conflict_when_compatible_and_not_when_they_diverge() {
    let overlapping = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Pattern(pattern("0987xxx"))),
        rule(2, Effect::Allow, Matcher::Pattern(pattern("098xxx7"))),
    ]);
    let diverging = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Pattern(pattern("0987xxx"))),
        rule(2, Effect::Allow, Matcher::Pattern(pattern("0512xxx"))),
    ]);

    assert_eq!(overlapping.conflicts().len(), 1);
    assert!(diverging.conflicts().is_empty());
}

#[test]
fn a_pattern_outranks_a_prefix_that_pins_the_same_digits() {
    // So the two can never tie, and conflicts() never pairs them. A pattern
    // fixes the length as well as the digits; a prefix also matches anything
    // longer.
    let p = Matcher::Pattern(pattern("0987xxx")).specificity();
    let s = Matcher::StartsWith(digits("0987")).specificity();

    assert_eq!(p.pinned(), s.pinned());
    assert!(p > s);
}

// can_overlap is a general predicate, not only the input to conflicts(). These
// cover the cross-kind pairs conflicts() can never present, because they cannot
// tie on specificity.
#[test]
fn overlap_across_kinds_is_decided_on_the_digits_that_could_coincide() {
    let pat = Matcher::Pattern(pattern("0987xxx"));

    assert!(pat.can_overlap(&Matcher::StartsWith(digits("0987"))));
    assert!(!pat.can_overlap(&Matcher::StartsWith(digits("0512"))));

    assert!(pat.can_overlap(&Matcher::EndsWith(digits("212"))));
    assert!(
        !pat.can_overlap(&Matcher::EndsWith(digits("21299"))),
        "a five-digit tail cannot fit a seven-atom pattern that already pins its head"
    );

    assert!(pat.can_overlap(&Matcher::Exact(digits("0987212"))));
    assert!(
        !pat.can_overlap(&Matcher::Exact(digits("0987212345"))),
        "the pattern fixes the length"
    );
}

#[test]
fn one_prefix_inside_another_overlaps_and_a_diverging_pair_does_not() {
    let broad = Matcher::StartsWith(digits("0987"));

    assert!(broad.can_overlap(&Matcher::StartsWith(digits("098777"))));
    assert!(!broad.can_overlap(&Matcher::StartsWith(digits("0512"))));
}

#[test]
fn a_caller_id_can_conflict_with_a_number_rule_only_when_equally_specific() {
    // A call carries both a number and a name, so both can match at once. But a
    // caller ID pins no digits, so it ties only with another caller ID.
    let two_names = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::CallerId("Acme".into())),
        rule(2, Effect::Allow, Matcher::CallerId("Acme".into())),
    ]);
    let name_and_number = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::CallerId("Acme".into())),
        rule(2, Effect::Allow, Matcher::StartsWith(digits("0987"))),
    ]);

    assert_eq!(two_names.conflicts().len(), 1);
    assert!(name_and_number.conflicts().is_empty());
}

#[test]
fn agreeing_rules_are_never_a_conflict() {
    let set = RuleSet::new(vec![
        rule(1, Effect::Deny, Matcher::Exact(digits("0987777212"))),
        rule(2, Effect::Deny, Matcher::Exact(digits("0987777212"))),
    ]);

    assert!(
        set.conflicts().is_empty(),
        "duplicates agree, so nothing to ask"
    );
}
