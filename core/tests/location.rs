//! A rule written against a place.
//!
//! Outside `core/src` because the §1 lint refuses digit runs in the shipped
//! crate.
//!
//! The case this file exists for: a place is not one prefix. "Turin" is three
//! area codes in the real Italian data and "Guangzhou, Guangdong" is 1,565 in
//! the Chinese. A model that resolved a place to a single prefix would block a
//! fraction of a city and look like it had worked.

use callerfilter_core::dataset::{Builder, Dataset, Kind};
use callerfilter_core::{
    evaluate, expand_rules_to_vec, explain, Budget, Call, Caveat, Decision, Digits, Effect,
    LocationRef, Matcher, Origin, PlatformVerdict as Verdict, PrefixSource, Prefixes, Rule, RuleId,
    RuleSet, Surface, E164,
};
use phonenumber::metadata::DATABASE;

fn dataset(language: &str, entries: &[(&str, &str)]) -> Dataset {
    let mut b = Builder::new();
    for (prefix, name) in entries {
        b.add(prefix, name);
    }
    Dataset::parse(&b.build(Kind::Places, language, "9.0.33").expect("builds")).expect("valid")
}

/// A city with three area codes, as Turin really has.
fn italian() -> Dataset {
    dataset(
        "it",
        &[
            ("39011", "Torino"),
            ("390122", "Torino"),
            ("390125", "Torino"),
            ("39010", "Genova"),
        ],
    )
}

fn english() -> Dataset {
    dataset("en", &[("39011", "Turin"), ("39010", "Genoa")])
}

fn places(sets: Vec<Dataset>) -> callerfilter_core::Places {
    let mut p = callerfilter_core::Places::new();
    for d in sets {
        p.push(d);
    }
    p
}

fn location_rule(id: u64, place: &str, language: &str, source: &dyn PrefixSource) -> Option<Rule> {
    let authored = callerfilter_core::Authored::Location(LocationRef::new(place, language));
    authored
        .resolve(source)
        .map(|m| Rule::from_location(RuleId(id), Effect::Deny, m))
}

fn blocks(rules: &RuleSet, number: &str) -> bool {
    let e = E164::new(number).expect("a number");
    evaluate(&Call::new(e), rules).decision == Decision::Block
}

// --------------------------------------------------------------- resolution

#[test]
fn a_place_with_three_area_codes_blocks_all_three() {
    // The bug this task exists to fix. Resolving to one prefix would have
    // blocked a third of Turin and reported success.
    let p = places(vec![italian()]);
    let rule = location_rule(1, "Torino", "it", &p).expect("resolves");
    let rules = RuleSet::new(vec![rule]);

    assert!(blocks(&rules, "+390111234567"), "first area code");
    assert!(blocks(&rules, "+390122123456"), "second area code");
    assert!(blocks(&rules, "+390125123456"), "third area code");
    assert!(!blocks(&rules, "+390101234567"), "Genova is not Turin");
}

#[test]
fn a_place_with_one_area_code_is_the_same_shape_with_a_set_of_one() {
    let p = places(vec![italian()]);
    let rule = location_rule(1, "Genova", "it", &p).expect("resolves");
    let rules = RuleSet::new(vec![rule]);

    assert!(blocks(&rules, "+390101234567"));
    assert!(!blocks(&rules, "+390111234567"));
}

#[test]
fn it_stays_one_rule_no_matter_how_many_prefixes_it_covers() {
    // The reason a prefix set was chosen over splitting into several rules:
    // R2 points the user at "the rule", explain reports per rule, and a
    // conflict names rule ids. Splitting would make all three describe
    // something the user never wrote.
    let p = places(vec![italian()]);
    let rule = location_rule(7, "Torino", "it", &p).expect("resolves");
    let rules = RuleSet::new(vec![rule]);

    assert_eq!(rules.len(), 1);
    let e = E164::new("+390122123456").expect("a number");
    assert_eq!(
        evaluate(&Call::new(e), &rules).matched.map(|r| r.id.0),
        Some(7)
    );
}

#[test]
fn a_place_the_data_no_longer_names_resolves_to_nothing() {
    // Legal, not an error: metadata changes and a place that resolved last
    // month may not today. The caller must say the rule stopped matching.
    let p = places(vec![italian()]);
    assert!(location_rule(1, "Atlantis", "it", &p).is_none());
}

#[test]
fn a_rule_written_in_one_language_still_resolves_after_switching_language() {
    // The name is the spelling the user picked. If that language is no longer
    // loaded, the name is tried against the others rather than the rule
    // silently dying.
    let english_only = places(vec![english()]);
    let rule = location_rule(1, "Torino", "it", &english_only);
    assert!(rule.is_none(), "English data has no 'Torino'");

    let both = places(vec![english(), italian()]);
    let rule = location_rule(1, "Torino", "it", &both).expect("found in the Italian data");
    let rules = RuleSet::new(vec![rule]);
    assert!(blocks(&rules, "+390111234567"));
}

#[test]
fn the_same_city_under_its_two_names_blocks_the_same_numbers() {
    let p = places(vec![italian(), english()]);

    let in_italian = RuleSet::new(vec![location_rule(1, "Torino", "it", &p).expect("resolves")]);
    let in_english = RuleSet::new(vec![location_rule(1, "Turin", "en", &p).expect("resolves")]);

    // English data only lists one of Turin's area codes, so the two are not
    // identical — which is itself worth knowing, and is a data property rather
    // than a bug in the model.
    assert!(blocks(&in_italian, "+390111234567"));
    assert!(blocks(&in_english, "+390111234567"));
    assert!(blocks(&in_italian, "+390122123456"));
    assert!(!blocks(&in_english, "+390122123456"));
}

#[test]
fn a_country_with_no_place_data_at_all_resolves_to_nothing() {
    let p = places(vec![]);
    assert!(location_rule(1, "Torino", "it", &p).is_none());
}

// -------------------------------------------------------------- specificity

#[test]
fn a_multi_prefix_rule_ranks_by_its_broadest_run() {
    // Turin's codes are five and six digits. Ranking by the longest would claim
    // a precision the rule does not have and let it beat something genuinely
    // narrower.
    let p = places(vec![italian()]);
    let turin = location_rule(1, "Torino", "it", &p).expect("resolves");

    let five = Rule::new(
        RuleId(2),
        Effect::Deny,
        Matcher::StartsWith(Prefixes::one(Digits::parse("39011").expect("digits"))),
    );
    let six = Rule::new(
        RuleId(3),
        Effect::Deny,
        Matcher::StartsWith(Prefixes::one(Digits::parse("390122").expect("digits"))),
    );

    assert_eq!(turin.specificity(), five.specificity());
    assert!(turin.specificity() < six.specificity());
}

#[test]
fn an_exact_allow_still_beats_a_location_deny() {
    // Most-specific-wins has to keep working across a multi-prefix rule.
    let p = places(vec![italian()]);
    let rules = RuleSet::new(vec![
        location_rule(1, "Torino", "it", &p).expect("resolves"),
        Rule::new(
            RuleId(2),
            Effect::Allow,
            Matcher::Exact(Digits::parse("390111234567").expect("digits")),
        ),
    ]);

    assert!(!blocks(&rules, "+390111234567"), "the exception");
    assert!(blocks(&rules, "+390119999999"), "the rest of the city");
}

// ---------------------------------------------------------------- expansion

#[test]
fn a_whole_city_is_too_broad_for_a_phone_and_is_not_silently_dropped() {
    // Turin's area codes leave six to nine digits free, which is over a billion
    // numbers. The rule works live on Android; it cannot be listed on iOS. That
    // is a real product fact, not a failure, and explain is what says so.
    let p = places(vec![italian()]);
    let rule = location_rule(1, "Torino", "it", &p).expect("resolves");
    let rules = RuleSet::new(vec![rule.clone()]);

    let surfaces = vec![Surface {
        id: "ios-calls".into(),
        evaluates_live: false,
        matches_caller_id: false,
        budget: Some(Budget::from_entry_limit(
            callerfilter_core::EntryLimit(1_800_000),
            10,
        )),
    }];

    match explain(&rule, &rules, &DATABASE, &surfaces).on("ios-calls") {
        Some(Verdict::TooBroad { upper_bound }) => assert!(*upper_bound > 1_800_000),
        other => panic!("expected too broad, got {other:?}"),
    }
}

#[test]
fn expansion_covers_every_prefix_and_stays_ascending() {
    // A country whose numbers have exactly one possible length, so the whole
    // set expands to something small enough to check entry by entry. Italy
    // allows seven lengths, which makes even a narrow prefix explode.
    let p = places(vec![dataset(
        "en",
        &[
            ("1202555", "Tiny"),
            ("1203555", "Tiny"),
            ("1205555", "Tiny"),
        ],
    )]);
    let rules = RuleSet::new(vec![location_rule(1, "Tiny", "en", &p).expect("resolves")]);

    let out = expand_rules_to_vec(
        &rules,
        &DATABASE,
        Budget::from_entry_limit(callerfilter_core::EntryLimit(2_000_000), 10),
    );

    assert!(!out.is_empty());
    assert!(out.windows(2).all(|w| w[0] < w[1]), "ascending, no repeats");

    let has = |lead: &str| out.iter().any(|n| n.to_string().starts_with(lead));
    assert!(has("1202555"), "first area code missing from the list");
    assert!(has("1203555"), "second area code missing from the list");
    assert!(has("1205555"), "third area code missing from the list");
}

#[test]
fn overlapping_prefixes_never_produce_a_duplicate() {
    // A place can name a code and a code inside it. The inner one is redundant
    // — every number under it is already under the outer — and keeping both
    // would emit those numbers twice, which iOS rejects the whole request for.
    let p = places(vec![dataset(
        "en",
        &[("1202555", "Somewhere"), ("12025551", "Somewhere")],
    )]);
    let rule = location_rule(1, "Somewhere", "en", &p).expect("resolves");

    match &rule.matcher {
        Matcher::StartsWith(prefixes) => assert_eq!(
            prefixes.len(),
            1,
            "the contained run is dropped, not kept alongside"
        ),
        other => panic!("expected a prefix set, got {other:?}"),
    }

    let rules = RuleSet::new(vec![rule]);

    let out = expand_rules_to_vec(
        &rules,
        &DATABASE,
        Budget::from_entry_limit(callerfilter_core::EntryLimit(2_000_000), 10),
    );

    assert!(out.windows(2).all(|w| w[0] < w[1]), "ascending, no repeats");
}

// ------------------------------------------------------------------ caveats

#[test]
fn explain_says_a_location_rule_cannot_match_a_mobile() {
    // Mobile numbering is not geographic anywhere, so this is true on every
    // platform including the ones that evaluate live. R2 requires saying it.
    let p = places(vec![italian()]);
    let rule = location_rule(1, "Torino", "it", &p).expect("resolves");
    let rules = RuleSet::new(vec![rule.clone()]);

    let surfaces = vec![Surface {
        id: "android-calls".into(),
        evaluates_live: true,
        matches_caller_id: true,
        budget: None,
    }];

    let e = explain(&rule, &rules, &DATABASE, &surfaces);

    assert_eq!(e.on("android-calls"), Some(&Verdict::AppliesLive));
    assert!(
        e.caveats.contains(&Caveat::LandlinesOnly),
        "the landline-only limit must reach the user"
    );
}

#[test]
fn a_hand_typed_prefix_carries_no_such_caveat() {
    let rule = Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::StartsWith(Prefixes::one(Digits::parse("39011").expect("digits"))),
    );
    let rules = RuleSet::new(vec![rule.clone()]);
    let surfaces = vec![Surface {
        id: "android-calls".into(),
        evaluates_live: true,
        matches_caller_id: true,
        budget: None,
    }];

    let e = explain(&rule, &rules, &DATABASE, &surfaces);

    assert_eq!(rule.origin, Origin::Direct);
    assert!(e.caveats.is_empty());
}

// --------------------------------------------------------- a very big place

#[test]
fn a_place_named_by_over_a_thousand_prefixes_still_works() {
    // "Guangzhou, Guangdong" is 1,565 prefixes in the real Chinese data.
    let entries: Vec<(String, String)> = (0..1200)
        .map(|i| (format!("8620{i:04}"), "Big City".to_string()))
        .collect();
    let borrowed: Vec<(&str, &str)> = entries
        .iter()
        .map(|(p, n)| (p.as_str(), n.as_str()))
        .collect();

    let p = places(vec![dataset("zh", &borrowed)]);
    let rule = location_rule(1, "Big City", "zh", &p).expect("resolves");

    match &rule.matcher {
        Matcher::StartsWith(prefixes) => assert_eq!(prefixes.len(), 1200),
        other => panic!("expected a prefix set, got {other:?}"),
    }

    let rules = RuleSet::new(vec![rule]);
    assert!(blocks(&rules, "+862000001234567"));
}
