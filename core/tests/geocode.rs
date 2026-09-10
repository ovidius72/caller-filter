//! Geocoding against real packed datasets.
//!
//! Outside `core/src` because the §1 lint refuses digit runs in the shipped
//! crate.
//!
//! These build their datasets in the test rather than reading
//! `datasets/build/`, so they do not depend on the converter having been run.
//! The fixtures are shaped like the real data — Italian covering Italy only,
//! English covering more — because that lopsidedness is what R7's fallback
//! exists for.

use callerfilter_core::dataset::{Builder, Dataset, Kind};
use callerfilter_core::{geocode, Located, Places};
use phonenumber::metadata::{Database, DATABASE};

fn db() -> &'static Database {
    &DATABASE
}

/// A dataset for one language, from `(prefix, name)` pairs.
fn dataset(language: &str, entries: &[(&str, &str)]) -> Dataset {
    let mut b = Builder::new();
    for (prefix, name) in entries {
        b.add(prefix, name);
    }
    Dataset::parse(&b.build(Kind::Places, language, "9.0.33")).expect("valid")
}

/// Italian names, covering Italy only — which is exactly its real coverage.
fn italian() -> Dataset {
    dataset("it", &[("39011", "Torino"), ("39010", "Genova")])
}

/// English names, covering Italy and the United Kingdom.
fn english() -> Dataset {
    dataset(
        "en",
        &[("39011", "Turin"), ("39010", "Genoa"), ("4420", "London")],
    )
}

fn places(datasets: Vec<Dataset>) -> Places {
    let mut p = Places::new();
    for d in datasets {
        p.push(d);
    }
    p
}

#[test]
fn a_landline_in_a_covered_country_gets_a_name() {
    let p = places(vec![english()]);
    assert_eq!(
        geocode("+390111234567", &p, db()),
        Located::Place {
            name: "Turin".into(),
            language: "en".into()
        }
    );
}

#[test]
fn a_mobile_is_reported_as_having_no_place_rather_than_no_data() {
    // The headline case. Italian mobiles start 3xx and the geocoding data has
    // nothing for them anywhere in the world, because mobile numbering is not
    // geographic. "Not found" would be the wrong thing to tell the user.
    let p = places(vec![english(), italian()]);
    assert_eq!(geocode("+393331234567", &p, db()), Located::NotGeographic);
}

#[test]
fn a_geographic_number_in_an_unpackaged_country_is_a_gap() {
    // Distinct from the mobile case: this one would have an answer if the
    // country were packaged.
    let p = places(vec![english()]);
    assert_eq!(geocode("+33123456789", &p, db()), Located::NoData);
}

#[test]
fn the_users_language_wins_when_it_covers_the_country() {
    let p = places(vec![italian(), english()]);
    assert_eq!(
        geocode("+390111234567", &p, db()),
        Located::Place {
            name: "Torino".into(),
            language: "it".into()
        }
    );
}

#[test]
fn english_answers_when_the_users_language_does_not_cover_the_country() {
    // Italian covers two countries in the real data, so this is the normal path
    // for an Italian user, not an edge case.
    let p = places(vec![italian(), english()]);
    assert_eq!(
        geocode("+442012345678", &p, db()),
        Located::Place {
            name: "London".into(),
            language: "en".into()
        }
    );
}

#[test]
fn the_result_says_which_language_it_used_so_the_ui_need_not_guess() {
    let p = places(vec![italian(), english()]);

    let home = geocode("+390111234567", &p, db());
    let away = geocode("+442012345678", &p, db());

    match (home, away) {
        (Located::Place { language: a, .. }, Located::Place { language: b, .. }) => {
            assert_eq!(a, "it");
            assert_eq!(b, "en");
        }
        other => panic!("expected two places, got {other:?}"),
    }
}

#[test]
fn with_no_datasets_loaded_everything_geographic_is_a_gap() {
    let p = Places::new();
    assert_eq!(geocode("+390111234567", &p, db()), Located::NoData);
    // Still knows a mobile is not a place: that comes from the number, not the
    // place data.
    assert_eq!(geocode("+393331234567", &p, db()), Located::NotGeographic);
}

#[test]
fn nonsense_is_reported_as_not_a_number() {
    let p = places(vec![english()]);
    assert_eq!(geocode("banana", &p, db()), Located::NotANumber);
    assert_eq!(geocode("", &p, db()), Located::NotANumber);
}

#[test]
fn a_number_too_short_to_match_any_prefix_is_a_gap_not_a_crash() {
    let p = places(vec![english()]);
    assert!(matches!(
        geocode("+391", &p, db()),
        Located::NoData | Located::NotANumber | Located::NotGeographic
    ));
}

#[test]
fn the_longest_prefix_decides_which_place() {
    // Two Italian cities differing in the last digit of their area code.
    let p = places(vec![english()]);

    let turin = geocode("+390111234567", &p, db());
    let genoa = geocode("+390101234567", &p, db());

    assert_ne!(turin, genoa);
    assert!(matches!(turin, Located::Place { ref name, .. } if name == "Turin"));
    assert!(matches!(genoa, Located::Place { ref name, .. } if name == "Genoa"));
}

#[test]
fn loading_order_is_preference_order() {
    let italian_first = places(vec![italian(), english()]);
    let english_first = places(vec![english(), italian()]);

    let a = geocode("+390111234567", &italian_first, db());
    let b = geocode("+390111234567", &english_first, db());

    assert!(matches!(a, Located::Place { ref name, .. } if name == "Torino"));
    assert!(matches!(b, Located::Place { ref name, .. } if name == "Turin"));
}

// ------------------------------------------------- geography without any data

#[test]
fn whether_a_number_is_somewhere_needs_no_place_data() {
    // A location rule cannot match a mobile, and the UI has to say that while
    // the rule is being written — long before any place name is looked up.
    use callerfilter_core::is_geographic;

    assert!(is_geographic("+390111234567", db()), "an Italian landline");
    assert!(!is_geographic("+393331234567", db()), "an Italian mobile");
    assert!(is_geographic("+442012345678", db()), "a British landline");
    assert!(!is_geographic("banana", db()));
}

#[test]
fn a_landline_that_keeps_a_leading_zero_is_still_a_place() {
    // Regression, and the reason this module does not use the crate's own
    // number_type: that builds the national number with value().to_string(),
    // which drops leading zeros, so it reports Unknown for every Italian
    // landline. Italy's leading zero is part of the number.
    use callerfilter_core::is_geographic;

    assert!(is_geographic("+390111234567", db()));
    assert!(is_geographic("+390212345678", db()));
}
