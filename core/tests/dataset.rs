//! Dataset container tests that need prefix-shaped fixtures.
//!
//! Outside `core/src` because the §1 lint refuses digit runs in the shipped
//! crate and an integration test binary ships with nothing.

use callerfilter_core::dataset::{Builder, Dataset, DatasetError, Kind, FORMAT_VERSION};

fn sample() -> Vec<u8> {
    let mut b = Builder::new();
    b.add("391", "One");
    b.add("3912", "Two");
    b.add("392", "Three");
    b.build(Kind::Places, "en", "9.0.33").expect("builds")
}

#[test]
fn a_dataset_round_trips() {
    let d = Dataset::parse(&sample()).expect("valid");
    assert_eq!(d.kind(), Kind::Places);
    assert_eq!(d.language(), "en");
    assert_eq!(d.upstream(), "9.0.33");
    assert_eq!(d.len(), 3);
}

#[test]
fn the_longest_matching_prefix_wins() {
    let d = Dataset::parse(&sample()).expect("valid");
    assert_eq!(d.lookup("39125"), Some("Two"));
    assert_eq!(d.lookup("39155"), Some("One"));
    assert_eq!(d.lookup("39255"), Some("Three"));
    assert_eq!(
        d.lookup("44155"),
        None,
        "an uncovered country finds nothing"
    );
}

#[test]
fn a_truncated_file_is_refused_at_every_cut() {
    // A dataset that loads but is short would just return nothing for the
    // prefixes that went missing, with no error anywhere.
    let whole = sample();
    for cut in [6, 12, 20, whole.len() - 1] {
        assert!(
            Dataset::parse(&whole[..cut]).is_err(),
            "a file cut at {cut} bytes must not parse"
        );
    }
}

#[test]
fn a_future_layout_is_refused_rather_than_misread() {
    let mut bytes = sample();
    bytes[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert_eq!(
        Dataset::parse(&bytes).unwrap_err(),
        DatasetError::UnsupportedFormat(FORMAT_VERSION + 1)
    );
}

#[test]
fn duplicate_length_groups_are_rejected_instead_of_overwritten() {
    let mut bytes = Builder::new().build(Kind::Places, "", "").unwrap();
    let at = bytes.len() - 2;
    bytes[at..].copy_from_slice(&2u16.to_le_bytes());
    for _ in 0..2 {
        bytes.push(1);
        bytes.extend_from_slice(&0u32.to_le_bytes());
    }
    assert_eq!(
        Dataset::parse(&bytes).unwrap_err(),
        DatasetError::DuplicateLength { length: 1 }
    );
}

#[test]
fn impossible_allocation_counts_and_unknown_kinds_are_rejected() {
    let mut bytes = Builder::new().build(Kind::Places, "", "").unwrap();
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Dataset::parse(&bytes).unwrap_err(),
        DatasetError::CountTooLarge
    );
    let mut bytes = sample();
    bytes[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        Dataset::parse(&bytes).unwrap_err(),
        DatasetError::UnknownKind(u16::MAX)
    );
}

#[test]
fn trailing_bytes_are_refused_rather_than_ignored() {
    let mut bytes = sample();
    bytes.extend_from_slice(b"garbage");
    assert_eq!(
        Dataset::parse(&bytes).unwrap_err(),
        DatasetError::TrailingBytes
    );
}
