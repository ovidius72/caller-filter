//! Turning what a user typed, or what the OS handed over, into one number.
//!
//! Everything downstream — matching, expansion, storage — works on E.164, so
//! this is the only place raw input exists. Two callers reach it and they do
//! not look alike:
//!
//! - The OS, when a call or message arrives. Android gives a `tel:` URI from
//!   `Call.Details`; iOS SMS gives `ILMessageFilterQueryRequest.sender`. These
//!   usually arrive already international.
//! - A person, writing a rule. They type the number the way they would dial it,
//!   which is usually national and has no country code in it at all.
//!
//! The second is why a default region exists. It is always a parameter: which
//! country the user is in is data the shell knows, never something this crate
//! may decide (Guidelines §1).

use std::str::FromStr;

use phonenumber::country;
use phonenumber::metadata::Database;
use phonenumber::{Mode, PhoneNumber};

use crate::evaluate::E164;

/// A number that parsed and is valid for its region.
///
/// Owns its text so callers can hold it; [`E164`] borrows, so evaluation can
/// take a reference without allocating on the call path.
///
/// Invariant: the string always starts with `+` and is otherwise digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Normalized(String);

impl Normalized {
    /// The number in E.164, leading `+` included.
    pub fn as_e164(&self) -> &str {
        &self.0
    }

    /// Borrow it for evaluation.
    pub fn number(&self) -> E164<'_> {
        E164::new(&self.0).expect("normalize only ever stores a valid E.164 number")
    }
}

impl AsRef<str> for Normalized {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Why a number could not be normalised.
///
/// These are deliberately separate because the user needs a different sentence
/// for each, and R2 forbids quietly accepting something that will never match.
/// The wording itself is not here — R7 makes UI text a data file, not code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizeError {
    /// Not a phone number at all. Letters, punctuation, far too few digits.
    NotANumber,
    /// It looks national, and no region was given to interpret it against.
    /// Asking which country is the only honest response; guessing one would
    /// silently produce a rule that matches the wrong numbers.
    RegionRequired,
    /// A region was given that the data does not know.
    UnknownRegion,
    /// It parsed, but it is not a valid number in that region. Accepting it
    /// would create a rule that can never match anything.
    NotValidForRegion,
}

/// Parse and validate one number.
///
/// `default_region` is a CLDR region code such as the one the device reports.
/// It is only consulted when the input is not already international; a number
/// that carries its own country code ignores it entirely.
///
/// The database is a parameter rather than the crate's bundled global because
/// R5 requires metadata to be updatable without an app release. Reaching for
/// the global here would close that off.
pub fn normalize(
    raw: &str,
    default_region: Option<&str>,
    database: &Database,
) -> Result<Normalized, NormalizeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NormalizeError::NotANumber);
    }

    let region = match default_region {
        Some(r) => Some(country::Id::from_str(r).map_err(|_| NormalizeError::UnknownRegion)?),
        None => None,
    };

    let already_international = trimmed.starts_with('+');

    // A number that does not announce its own country cannot be read without
    // one. Say so rather than letting the parser guess or fail obscurely.
    if region.is_none() && !already_international {
        return Err(NormalizeError::RegionRequired);
    }

    // Withhold the region from a number that already carries its own country
    // code. The parser applies the hint region's national-prefix rule even to
    // an international number, and countries disagree about the trunk digit:
    // Italy keeps a leading 0 in the national number, Germany strips one. So a
    // valid Italian number parsed with a German hint loses its 0 and then fails
    // validation. That would tell a user abroad their own home number does not
    // exist. The region is for reading a national number, and this is not one.
    let hint = if already_international { None } else { region };

    let parsed: PhoneNumber =
        phonenumber::parse_with(database, hint, trimmed).map_err(|_| NormalizeError::NotANumber)?;

    if !phonenumber::is_valid_with(database, &parsed) {
        return Err(NormalizeError::NotValidForRegion);
    }

    Ok(Normalized(
        phonenumber::format(&parsed).mode(Mode::E164).to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonenumber::metadata::DATABASE;

    // Number-shaped fixtures live in core/tests/. These cover only the cases
    // that need no digits.

    #[test]
    fn empty_and_blank_input_is_not_a_number() {
        assert_eq!(
            normalize("", None, &DATABASE),
            Err(NormalizeError::NotANumber)
        );
        assert_eq!(
            normalize("   ", None, &DATABASE),
            Err(NormalizeError::NotANumber)
        );
    }

    #[test]
    fn a_region_the_data_does_not_know_is_refused() {
        assert_eq!(
            normalize("123", Some("ZZ"), &DATABASE),
            Err(NormalizeError::UnknownRegion)
        );
    }

    #[test]
    fn letters_are_not_a_number() {
        assert_eq!(
            normalize("+call me", None, &DATABASE),
            Err(NormalizeError::NotANumber)
        );
    }
}
