//! Normalization tests.
//!
//! Outside `core/src` because the Guidelines §1 lint refuses digit runs in the
//! shipped crate and an integration test binary ships with nothing.
//!
//! The regions here are fixtures, not choices the product makes. Every one of
//! them arrives as a parameter, which is the whole point: the core never picks
//! a country.

use callerfilter_core::{
    evaluate, normalize, Call, Decision, Digits, Effect, Matcher, NormalizeError, Rule, RuleId,
    RuleSet,
};
use phonenumber::metadata::DATABASE;

fn norm(raw: &str, region: Option<&str>) -> Result<String, NormalizeError> {
    normalize(raw, region, &DATABASE).map(|n| n.as_e164().to_string())
}

// --------------------------------------------------------------- the happy path

#[test]
fn a_number_already_international_needs_no_region() {
    assert_eq!(norm("+390212345678", None).unwrap(), "+390212345678");
}

#[test]
fn a_national_number_is_read_against_the_region_it_is_given() {
    assert_eq!(norm("0212345678", Some("IT")).unwrap(), "+390212345678");
}

#[test]
fn the_separators_people_actually_type_are_accepted() {
    let spaced = norm("02 1234 5678", Some("IT")).unwrap();
    let dotted = norm("02.1234.5678", Some("IT")).unwrap();
    let dashed = norm(" (02) 1234-5678 ", Some("IT")).unwrap();

    assert_eq!(spaced, "+390212345678");
    assert_eq!(dotted, spaced);
    assert_eq!(dashed, spaced);
}

#[test]
fn an_international_number_ignores_a_region_that_disagrees_with_it() {
    // Regression. Handing the parser a region hint alongside an already
    // international number makes it apply that region's national-prefix rule:
    // with a German hint the Italian number lost its leading 0, became
    // 212345678, and failed validation. A user in Germany would have been told
    // their own Italian number does not exist.
    //
    // Both directions, because the trunk digit is what differs — Italy keeps a
    // leading 0 in the national number and Germany strips one.
    assert_eq!(norm("+390212345678", Some("DE")).unwrap(), "+390212345678");
    assert_eq!(norm("+493012345678", Some("IT")).unwrap(), "+493012345678");
}

// ------------------------------------------------------------------- refusals

#[test]
fn a_national_number_with_no_region_is_refused_rather_than_guessed() {
    // Guessing would silently produce a rule matching some other country's
    // numbers, which R2 forbids outright.
    assert_eq!(
        norm("0212345678", None),
        Err(NormalizeError::RegionRequired)
    );
}

#[test]
fn a_number_that_is_not_valid_in_its_region_is_refused() {
    // It parses, but no such number exists, so a rule built on it could never
    // match. That is a different sentence for the user than "not a number".
    assert_eq!(
        norm("+3902123", None),
        Err(NormalizeError::NotValidForRegion)
    );
}

#[test]
fn the_two_refusals_are_distinguishable() {
    let nonsense = norm("banana", Some("IT"));
    let unmatched = norm("+3902123", None);

    assert_eq!(nonsense, Err(NormalizeError::NotANumber));
    assert_eq!(unmatched, Err(NormalizeError::NotValidForRegion));
    assert_ne!(nonsense, unmatched, "the UI says something different");
}

#[test]
fn a_number_valid_in_one_region_can_be_invalid_in_another() {
    // The same digits mean different things depending on where they are dialled.
    let at_home = norm("0212345678", Some("IT"));
    let elsewhere = norm("0212345678", Some("DE"));

    assert!(at_home.is_ok());
    assert_ne!(at_home, elsewhere);
}

#[test]
fn a_short_code_is_not_a_dialable_number() {
    // Short codes are not valid E.164 numbers, and a rule against one could
    // never match a call. Refused rather than stored as if it worked.
    assert!(norm("112", Some("IT")).is_err());
}

// ----------------------------------------------------------------- end to end

#[test]
fn a_normalized_number_can_be_evaluated_without_allocating_again() {
    // The point of Normalized owning its text and E164 borrowing: the call path
    // takes a reference and allocates nothing.
    let rules = RuleSet::new(vec![
        Rule::new(
            RuleId(1),
            Effect::Deny,
            Matcher::StartsWith(Digits::parse("3902").expect("digits")),
        ),
        Rule::new(
            RuleId(2),
            Effect::Allow,
            Matcher::Exact(Digits::parse("390212345678").expect("digits")),
        ),
    ]);

    let blocked = normalize("02 9999 9999", Some("IT"), &DATABASE).expect("valid");
    let allowed = normalize("02 1234 5678", Some("IT"), &DATABASE).expect("valid");

    assert_eq!(
        evaluate(&Call::new(blocked.number()), &rules).decision,
        Decision::Block
    );
    assert_eq!(
        evaluate(&Call::new(allowed.number()), &rules).decision,
        Decision::Allow,
        "the exact allow beats the range deny, through the whole pipeline"
    );
}

#[test]
fn the_same_number_written_three_ways_evaluates_identically() {
    // A rule the user typed nationally must match a call the OS delivers
    // internationally. This is the reason normalization exists at all.
    let rules = RuleSet::new(vec![Rule::new(
        RuleId(1),
        Effect::Deny,
        Matcher::Exact(Digits::parse("390212345678").expect("digits")),
    )]);

    for (raw, region) in [
        ("+390212345678", None),
        ("0212345678", Some("IT")),
        ("02 1234 5678", Some("IT")),
    ] {
        let n = normalize(raw, region, &DATABASE).expect("valid");
        assert_eq!(
            evaluate(&Call::new(n.number()), &rules).decision,
            Decision::Block,
            "{raw} should reach the same verdict"
        );
    }
}
