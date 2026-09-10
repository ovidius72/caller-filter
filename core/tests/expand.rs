//! Expansion tests.
//!
//! Outside `core/src` because the Guidelines §1 lint refuses digit runs in the
//! shipped crate and an integration test binary ships with nothing.

use callerfilter_core::{
    expand_matcher, expand_rules_to_vec, Budget, Digits, Effect, EntryLimit, Expansion, Matcher,
    Pattern, Rule, RuleId, RuleSet,
};
use phonenumber::metadata::DATABASE;

fn digits(s: &str) -> Digits {
    Digits::parse(s).expect("valid digits")
}

fn budget(entries: u64) -> Budget {
    Budget {
        max_entries: entries,
        max_candidates: entries.saturating_mul(10),
    }
}

fn collect(matcher: &Matcher, b: Budget) -> Vec<i64> {
    match expand_matcher(matcher, &DATABASE, b) {
        Expansion::Fits(n) => n.collect(),
        other => panic!("expected it to fit, got {other:?}"),
    }
}

fn verdict(matcher: &Matcher, b: Budget) -> String {
    match expand_matcher(matcher, &DATABASE, b) {
        Expansion::Fits(n) => format!("fits({})", n.count()),
        Expansion::TooBroad { upper_bound } => format!("too_broad({upper_bound})"),
        Expansion::NotExpandable(r) => format!("not_expandable({r:?})"),
    }
}

// ------------------------------------------------------- what cannot expand

#[test]
fn a_caller_id_rule_cannot_become_a_list_of_numbers() {
    // It works on Android and on iOS messages. On iOS calls there is no name to
    // match, so the user has to be told rather than left wondering.
    assert_eq!(
        verdict(&Matcher::CallerId("Acme".into()), budget(1000)),
        "not_expandable(CallerId)"
    );
}

#[test]
fn a_suffix_rule_is_refused_rather_than_attempted() {
    // Finding every number that ends a given way means walking the whole
    // country. Refused up front instead of timing out.
    assert_eq!(
        verdict(&Matcher::EndsWith(digits("7212")), budget(1000)),
        "not_expandable(Suffix)"
    );
}

#[test]
fn digits_belonging_to_no_country_code_cannot_expand() {
    assert_eq!(
        verdict(&Matcher::StartsWith(digits("9999999")), budget(1000)),
        "not_expandable(UnknownCountry)"
    );
}

#[test]
fn a_pattern_whose_leading_digits_vary_belongs_to_no_country() {
    let p = Pattern::parse("xx0212345").expect("valid pattern");
    assert_eq!(
        verdict(&Matcher::Pattern(p), budget(1000)),
        "not_expandable(UnknownCountry)"
    );
}

// ------------------------------------------------------------ exact numbers

#[test]
fn an_exact_number_expands_to_itself() {
    let out = collect(&Matcher::Exact(digits("390212345678")), budget(1000));
    assert_eq!(out, vec![390_212_345_678]);
}

#[test]
fn an_exact_number_that_does_not_exist_expands_to_nothing() {
    // The validating pattern is what makes this empty rather than one entry.
    let out = collect(&Matcher::Exact(digits("391112345678")), budget(1000));
    assert!(out.is_empty(), "got {out:?}");
}

// ------------------------------------------------------------------ ordering

#[test]
fn numbers_come_out_strictly_ascending_with_no_duplicates() {
    // iOS rejects the whole request if an entry repeats or arrives out of
    // order, so this is generated in order rather than sorted afterwards.
    let p = Pattern::parse("39021234xxx").expect("valid pattern");
    let out = collect(&Matcher::Pattern(p), budget(10_000));

    assert!(!out.is_empty());
    assert!(
        out.windows(2).all(|w| w[0] < w[1]),
        "not strictly ascending"
    );
}

#[test]
fn a_prefix_walks_shorter_numbers_before_longer_ones() {
    // Every number here shares a prefix, so a longer one is always the larger.
    // Walking lengths in order is therefore already numerically ascending.
    let out = collect(&Matcher::StartsWith(digits("39021234567")), budget(200_000));

    assert!(out.windows(2).all(|w| w[0] < w[1]));
    let shortest = out.first().copied().unwrap().to_string().len();
    let longest = out.last().copied().unwrap().to_string().len();
    assert!(longest > shortest, "several lengths should be represented");
}

// --------------------------------------------------------------- the budget

#[test]
fn a_prefix_wider_than_the_budget_is_reported_with_a_real_figure() {
    // Italy's fixed lines run to twelve digits, so a two-digit area code covers
    // far more numbers than any phone will hold. The figure shown is the
    // arithmetic ceiling from the possible lengths, which is the true count
    // wherever the pattern leaves the tail alone — as it does here.
    let v = verdict(&Matcher::StartsWith(digits("3902")), budget(1_000));
    assert!(v.starts_with("too_broad("), "got {v}");
}

#[test]
fn the_measured_ios_limit_is_injected_not_assumed() {
    // The budget comes from what was measured on a device, never a constant in
    // this crate. Same rule, two budgets, two answers.
    let matcher = Matcher::StartsWith(digits("39021234567"));

    let generous = Budget::from_entry_limit(EntryLimit::default(), 10);
    let mean = Budget::from_entry_limit(EntryLimit(10), 10);

    assert!(matches!(
        expand_matcher(&matcher, &DATABASE, generous),
        Expansion::Fits(_)
    ));
    assert!(matches!(
        expand_matcher(&matcher, &DATABASE, mean),
        Expansion::TooBroad { .. }
    ));
}

#[test]
fn counting_stops_early_instead_of_counting_to_eleven_trillion() {
    // German fixed lines have eleven possible lengths, 5 to 15, and the
    // validating pattern accepts every completion under a viable prefix. So the
    // honest count really is astronomical, and it has to be arrived at by
    // arithmetic rather than by enumeration. If this test hangs, that broke.
    let started = std::time::Instant::now();
    let v = verdict(&Matcher::StartsWith(digits("4930")), budget(1_800_000));
    let elapsed = started.elapsed();

    assert!(v.starts_with("too_broad("), "got {v}");
    assert!(
        elapsed.as_secs() < 5,
        "took {elapsed:?} — it is enumerating when it should be counting"
    );
}

// ---------------------------------------------------- what the pattern does

#[test]
fn the_validating_pattern_rejects_numbers_that_do_not_exist() {
    // This is what applying the pattern buys: not a smaller count, but a list
    // with nothing fictional in it. Compare a viable area code against one that
    // no Italian number uses.
    let real = collect(&Matcher::StartsWith(digits("39021234567")), budget(200_000));
    let fake = collect(&Matcher::StartsWith(digits("39111234567")), budget(200_000));

    assert!(!real.is_empty());
    assert!(
        fake.is_empty(),
        "an unused area code must expand to nothing, got {} entries",
        fake.len()
    );
}

// -------------------------------------------------------- allow subtraction

#[test]
fn an_allow_inside_a_deny_removes_exactly_that_number() {
    let deny_only = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Pattern(Pattern::parse("39021234567x").expect("pattern")),
    )]);
    let with_exception = RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("39021234567x").expect("pattern")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Allow,
            Matcher::Exact(digits("390212345678")),
        ),
    ]);

    let all = expand_rules_to_vec(&deny_only, &DATABASE, budget(10_000));
    let carved = expand_rules_to_vec(&with_exception, &DATABASE, budget(10_000));

    assert!(!all.is_empty());
    assert_eq!(
        carved.len(),
        all.len() - 1,
        "the exception costs exactly one entry"
    );
    assert!(!carved.contains(&390_212_345_678));
    assert!(all.contains(&390_212_345_678));
}

#[test]
fn an_allow_with_no_enclosing_deny_expands_to_nothing() {
    // Absence from the list already means allowed, so there is nothing to emit.
    let rules = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Allow,
        Matcher::Exact(digits("390212345678")),
    )]);

    assert!(expand_rules_to_vec(&rules, &DATABASE, budget(10_000)).is_empty());
}

#[test]
fn two_overlapping_denies_never_emit_the_same_number_twice() {
    // iOS rejects the whole request on a repeated entry.
    let rules = RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("3902123456xx").expect("pattern")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("39021234567x").expect("pattern")),
        ),
    ]);

    let out = expand_rules_to_vec(&rules, &DATABASE, budget(10_000));

    assert!(!out.is_empty());
    assert!(out.windows(2).all(|w| w[0] < w[1]), "ascending, no repeats");
}

#[test]
fn the_list_ios_gets_agrees_with_what_android_decides_live() {
    // Precedence is not decided twice. Every candidate goes through evaluate,
    // so a number on the iOS list is exactly one Android would block.
    use callerfilter_core::{evaluate, Call, Decision, E164};

    let rules = RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::Pattern(Pattern::parse("39021234567x").expect("pattern")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Allow,
            Matcher::Exact(digits("390212345678")),
        ),
    ]);

    for n in expand_rules_to_vec(&rules, &DATABASE, budget(10_000)) {
        let text = format!("+{n}");
        let e164 = E164::new(&text).expect("generated numbers are well formed");
        assert_eq!(
            evaluate(&Call::new(e164), &rules).decision,
            Decision::Block,
            "{n} is on the iOS list but Android would not block it"
        );
    }
}
