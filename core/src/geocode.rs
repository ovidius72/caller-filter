//! Where a number is from.
//!
//! Used to show what a number is when a call arrives or a rule is written, and
//! by the location half of a rule (P004(F002)/T017).
//!
//! Runs in the app, not in an extension — see the note in [`crate::dataset`].
//!
//! # Two things the caller must be able to tell apart
//!
//! **A mobile has no place.** Mobile numbering is not geographic anywhere, so
//! there is no data for it and never will be. Measured 2026-09-10: Italy has 0
//! geocoding entries for `393` and 121 for `390`. Reporting "not found" for a
//! mobile would be a lie by omission — the answer exists, and it is "this
//! question does not apply".
//!
//! **A place name is approximate.** Numbers are portable: a landline can keep
//! its number after moving. The data says where the number was issued, not
//! where the caller is.

use phonenumber::metadata::Database;
use phonenumber::PhoneNumber;

use crate::dataset::Dataset;
use crate::expand::descriptors_of;
use crate::rule::{Digits, LocationRef, PrefixSource};

/// Where a number is from, or why that cannot be said.
///
/// Three different answers, kept apart because they are three different things
/// to tell someone. Flattening them into an `Option<String>` would leave the UI
/// unable to explain a blank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// A place, and the language its name came from — which is not always the
    /// one that was asked for.
    ///
    /// Approximate. The number was issued there; the caller may not be.
    Place { name: String, language: String },
    /// The number is a mobile, or another kind that is not tied to anywhere.
    /// There is no data for these and there never will be.
    NotGeographic,
    /// Geographic, but nothing in the loaded data covers this prefix. Either
    /// the country is not packaged, or no language loaded has it.
    NoData,
    /// The number could not be read at all.
    NotANumber,
}

/// The place datasets in use, in the order they should be tried.
///
/// The core never opens a file: the shells own that, and an iOS extension and
/// an Android app disagree about where bytes come from. Hand it parsed
/// datasets.
#[derive(Debug, Default)]
pub struct Places {
    datasets: Vec<Dataset>,
}

impl Places {
    pub fn new() -> Self {
        Places::default()
    }

    /// Add a dataset. Order is preference order: the user's language first,
    /// then English.
    ///
    /// R7 requires that fallback, and it is the normal path rather than an edge
    /// case — English covers 151 countries, Italian covers 2.
    pub fn push(&mut self, dataset: Dataset) -> &mut Self {
        self.datasets.push(dataset);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.datasets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.datasets.len()
    }

    /// The languages loaded, in preference order.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.datasets.iter().map(|d| d.language())
    }

    /// Every place name offered, for the language given, so a picker can list
    /// them. Empty when that language is not loaded.
    pub fn names_in(&self, language: &str) -> Vec<&str> {
        self.datasets
            .iter()
            .find(|d| d.language() == language)
            .map(|d| d.names().collect())
            .unwrap_or_default()
    }

    /// First dataset that knows this prefix, and what it calls it.
    fn lookup(&self, digits: &str) -> Option<(&str, &str)> {
        self.datasets
            .iter()
            .find_map(|d| d.lookup(digits).map(|name| (name, d.language())))
    }
}

/// Whether a number is tied to a place, as far as the data can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Geography {
    /// A fixed line. Issued somewhere.
    Geographic,
    /// A mobile or a service number. Not tied anywhere, and no data ever will
    /// be.
    NotGeographic,
    /// Nothing in the metadata recognises it, so no claim either way.
    Unknown,
}

/// Work out whether a number is somewhere.
///
/// Deliberately not `PhoneNumber::number_type`. That builds the national number
/// with `value().to_string()`, which drops leading zeros, so it returns
/// `Unknown` for every Italian landline — Italy keeps a significant leading
/// zero. The crate's own `is_valid_with` uses `Display` and gets it right, so
/// the two disagree: a number can be valid and typeless at once. This uses the
/// `Display` form, which is the correct national significant number.
///
/// A fixed line means somewhere. Anything else the metadata recognises —
/// mobile, toll free, premium rate and the rest — means nowhere. Where a
/// territory cannot tell fixed from mobile apart, the fixed-line pattern
/// matches and the number counts as geographic, which is the useful answer
/// rather than refusing to look.
fn geography(number: &PhoneNumber, database: &Database) -> Geography {
    // Display keeps leading zeros; value() does not. That difference is the
    // whole reason this function exists.
    let national = number.national().to_string();
    let Some(metas) = database.by_code(&number.country().code()) else {
        return Geography::Unknown;
    };

    let mut recognised = false;
    for meta in metas {
        if meta
            .descriptors()
            .fixed_line()
            .is_some_and(|d| d.is_match(&national))
        {
            return Geography::Geographic;
        }
        if descriptors_of(meta).iter().any(|d| d.is_match(&national)) {
            recognised = true;
        }
    }

    if recognised {
        Geography::NotGeographic
    } else {
        Geography::Unknown
    }
}

/// Whether a number is tied to a place at all, without needing any place data.
///
/// Separate from [`geocode`] because the answer is useful on its own: a rule
/// that filters by location cannot match a mobile, and the UI has to say so
/// when the rule is written rather than when a call slips through.
pub fn is_geographic(number: &str, database: &Database) -> bool {
    phonenumber::parse_with(database, None, number)
        .map(|p| geography(&p, database) == Geography::Geographic)
        .unwrap_or(false)
}

/// Where a number is from.
///
/// `number` is E.164, with or without its leading `+`. The database supplies
/// the number's type; `places` supplies the names.
pub fn geocode(number: &str, places: &Places, database: &Database) -> Located {
    let Ok(parsed) = phonenumber::parse_with(database, None, number) else {
        return Located::NotANumber;
    };

    // Ask what kind of number it is before looking anything up. A mobile and an
    // uncovered country both find nothing, and they are not the same answer.
    match geography(&parsed, database) {
        Geography::NotGeographic => return Located::NotGeographic,
        // Nothing recognises it, so claiming it is nowhere would overstate what
        // is known. Look it up and let the data answer.
        Geography::Unknown | Geography::Geographic => {}
    }

    let digits = digits_of(&parsed);
    match places.lookup(&digits) {
        Some((name, language)) => Located::Place {
            name: name.to_string(),
            language: language.to_string(),
        },
        None => Located::NoData,
    }
}

/// The full E.164 digits, country code included, without the `+`.
///
/// The datasets are keyed that way because a prefix is only meaningful with its
/// country code in front of it.
fn digits_of(number: &PhoneNumber) -> String {
    // `national()` renders through Display, which keeps a significant leading
    // zero. `value()` would drop it and key Italian numbers wrongly.
    format!("{}{}", number.country().code(), number.national())
}

impl PrefixSource for Places {
    /// Every prefix that names this place.
    ///
    /// Looks in the language the rule was written in first, because that is the
    /// spelling the user picked. If that language is no longer loaded — the app
    /// changed language, or packaging changed — the name is tried against every
    /// other loaded dataset, so a rule written in Italian keeps working for a
    /// user who switched to English.
    ///
    /// An empty result is a real answer, not an error. Metadata changes, and a
    /// place that resolved last month may not today; the caller has to say the
    /// rule stopped matching rather than pretend it still does.
    fn prefixes_for(&self, location: &LocationRef) -> Vec<Digits> {
        let preferred = self
            .datasets
            .iter()
            .filter(|d| d.language() == location.language);
        let rest = self
            .datasets
            .iter()
            .filter(|d| d.language() != location.language);

        preferred
            .chain(rest)
            .map(|d| d.prefixes_named(&location.name))
            .find(|found| !found.is_empty())
            .unwrap_or_default()
            .iter()
            .filter_map(|p| Digits::parse(p).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Number-shaped fixtures live in core/tests/geocode.rs, where the §1 lint
    // does not reach. Deciding a number's geography needs real numbers, so
    // those tests live there in full.

    #[test]
    fn no_places_loaded_is_a_gap_not_a_crash() {
        let places = Places::new();
        assert!(places.is_empty());
        assert_eq!(places.len(), 0);
        assert_eq!(places.lookup("391"), None);
    }
}
