//! The whole pipeline against the real packed data.
//!
//! Everything else in the geocoding tests uses fixtures, which proves the logic
//! but not that it agrees with what libphonenumber actually ships. This reads
//! the built datasets.
//!
//! Ignored by default because it needs the converter to have run first:
//!
//! ```text
//! cargo run -p callerfilter-datasets --bin build-datasets -- it en
//! cargo test --test real_check -- --ignored --nocapture
//! ```

use callerfilter_core::dataset::Dataset;
use callerfilter_core::{geocode, Located, Places};
use phonenumber::metadata::DATABASE;

/// Built datasets live at the workspace root; a test's working directory is its
/// own crate.
fn built(language: &str) -> Option<Dataset> {
    let path = format!(
        "{}/../datasets/build/places.{language}.cfds",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).ok()?;
    Dataset::parse(&bytes).ok()
}

#[test]
#[ignore = "needs build-datasets to have run; see the module docs"]
fn real_italian_data_names_real_italian_cities() {
    let mut places = Places::new();
    for language in ["it", "en"] {
        let Some(d) = built(language) else {
            eprintln!("no dataset for {language}; run build-datasets first");
            return;
        };
        places.push(d);
    }

    // Italian landlines, named in Italian because Italian covers Italy.
    for (number, expected) in [
        ("+390111234567", "Torino"),
        ("+390101234567", "Genova"),
        ("+390212345678", "Milano"),
    ] {
        assert_eq!(
            geocode(number, &places, &DATABASE),
            Located::Place {
                name: expected.to_string(),
                language: "it".to_string()
            },
            "{number}"
        );
    }

    // An Italian mobile has no place, and that is an answer rather than a gap.
    assert_eq!(
        geocode("+393331234567", &places, &DATABASE),
        Located::NotGeographic
    );

    // Italian covers two countries, so a British number falls back to English.
    // This is the normal path for most users, not an edge case.
    assert_eq!(
        geocode("+442012345678", &places, &DATABASE),
        Located::Place {
            name: "London".to_string(),
            language: "en".to_string()
        }
    );
}
