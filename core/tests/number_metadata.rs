use callerfilter_core::number_metadata::{
    NumberMetadata, NumberMetadataError, FORMAT_VERSION, MAGIC,
};

#[path = "../../tools/ffi-tests/fixtures.rs"]
mod fixtures;

#[test]
fn v1_nonempty_golden_remains_readable_and_byte_identical() {
    let hex = include_str!("../../tools/ffi-tests/numbering-v1.hex").trim();
    let frozen: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(
        fixtures::numbering(),
        frozen,
        "DTO change requires a schema decision, not fixture regeneration"
    );
    let parsed = NumberMetadata::parse(&frozen).unwrap();
    assert_eq!(parsed.upstream(), "future-test-version");
    assert!(parsed.database().by_id("IT").is_some());
}

#[test]
fn every_optional_descriptor_is_validated_before_dependency_conversion() {
    for index in 0..16 {
        let mut metadata = fixtures::metadata();
        let m = &mut metadata[0];
        let fields = [
            &mut m.general,
            &mut m.fixed_line,
            &mut m.mobile,
            &mut m.toll_free,
            &mut m.premium_rate,
            &mut m.shared_cost,
            &mut m.personal_number,
            &mut m.voip,
            &mut m.pager,
            &mut m.uan,
            &mut m.emergency,
            &mut m.voicemail,
            &mut m.short_code,
            &mut m.standard_rate,
            &mut m.carrier,
            &mut m.no_international,
        ];
        *fields.into_iter().nth(index).unwrap() = Some(Default::default());
        let bytes = NumberMetadata::build("test", metadata).unwrap();
        assert_eq!(
            NumberMetadata::parse(&bytes).unwrap_err(),
            NumberMetadataError::InvalidDatabase
        );
    }
}

#[test]
fn repeated_non_geographic_ids_on_distinct_codes_are_accepted() {
    let mut metadata = fixtures::metadata();
    metadata[0].id = Some("001".into());
    metadata[0].country_code = Some(800);
    let mut second = metadata[0].clone();
    second.country_code = Some(808);
    metadata.push(second);
    let bytes = NumberMetadata::build("test", metadata).unwrap();
    assert!(NumberMetadata::parse(&bytes).is_ok());
}

#[test]
fn duplicate_geographic_id_on_different_codes_cannot_overwrite_hint_lookup() {
    let mut metadata = fixtures::metadata();
    let mut second = metadata[0].clone();
    second.country_code = Some(44);
    metadata.push(second);
    let bytes = NumberMetadata::build("test", metadata).unwrap();
    assert_eq!(
        NumberMetadata::parse(&bytes).unwrap_err(),
        NumberMetadataError::InvalidDatabase
    );
}

#[test]
fn unknown_shared_code_region_is_rejected_instead_of_panicking_later() {
    let mut metadata = fixtures::metadata();
    let mut second = metadata[0].clone();
    second.id = Some("ZZ".into());
    metadata.push(second);
    let bytes = NumberMetadata::build("test", metadata).unwrap();
    assert_eq!(
        NumberMetadata::parse(&bytes).unwrap_err(),
        NumberMetadataError::UnsupportedRegion {
            region: "ZZ".into()
        }
    );
}

#[test]
fn corrupt_patterns_and_duplicate_region_code_pairs_are_refused() {
    let mut metadata = fixtures::metadata();
    metadata[0].general.as_mut().unwrap().national_number = Some("[".into());
    let bytes = NumberMetadata::build("test", metadata).unwrap();
    assert_eq!(
        NumberMetadata::parse(&bytes).unwrap_err(),
        NumberMetadataError::InvalidDatabase
    );
    let mut metadata = fixtures::metadata();
    metadata.push(metadata[0].clone());
    let bytes = NumberMetadata::build("test", metadata).unwrap();
    assert_eq!(
        NumberMetadata::parse(&bytes).unwrap_err(),
        NumberMetadataError::InvalidDatabase
    );
}

#[test]
fn empty_number_metadata_round_trips_as_a_valid_database() {
    let bytes = NumberMetadata::build("test", Vec::new()).expect("frame");
    let parsed = NumberMetadata::parse(&bytes).expect("parse");
    assert_eq!(parsed.upstream(), "test");
}

#[test]
fn wrong_version_and_trailing_bytes_are_rejected() {
    let mut bytes = NumberMetadata::build("test", Vec::new()).expect("frame");
    bytes[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert!(
        matches!(NumberMetadata::parse(&bytes), Err(NumberMetadataError::UnsupportedFormat(v)) if v == FORMAT_VERSION + 1)
    );

    let mut bytes = NumberMetadata::build("test", Vec::new()).expect("frame");
    bytes.extend_from_slice(b"extra");
    assert!(matches!(
        NumberMetadata::parse(&bytes),
        Err(NumberMetadataError::TrailingBytes)
    ));

    // The frame itself may claim the tail as payload; postcard must still
    // reject bytes left after the decoded DTO.
    let payload_len_at = 4 + 2 + 4 + 4;
    let old_len = u32::from_le_bytes(
        bytes[payload_len_at..payload_len_at + 4]
            .try_into()
            .unwrap(),
    );
    bytes[payload_len_at..payload_len_at + 4].copy_from_slice(&(old_len + 5).to_le_bytes());
    assert!(matches!(
        NumberMetadata::parse(&bytes),
        Err(NumberMetadataError::InvalidPayload)
    ));
    assert_eq!(&bytes[..4], MAGIC);
}

#[test]
fn missing_descriptor_pattern_is_an_error_not_a_panic() {
    use phonenumber::metadata::loader::{Descriptor, Metadata};
    let metadata = Metadata {
        id: Some("IT".into()),
        country_code: Some(39),
        general: Some(Descriptor::default()),
        ..Default::default()
    };
    let bytes = NumberMetadata::build("test", vec![metadata]).unwrap();
    assert!(matches!(
        NumberMetadata::parse(&bytes),
        Err(NumberMetadataError::InvalidDatabase)
    ));
}

#[test]
fn truncation_is_rejected() {
    let bytes = NumberMetadata::build("test", Vec::new()).expect("frame");
    for end in 0..bytes.len() {
        assert!(matches!(
            NumberMetadata::parse(&bytes[..end]),
            Err(NumberMetadataError::Truncated
                | NumberMetadataError::InvalidPayload
                | NumberMetadataError::InvalidDatabase)
        ));
    }
}
