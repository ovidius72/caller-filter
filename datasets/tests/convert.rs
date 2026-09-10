//! Converter tests, against source trees written here rather than the vendored
//! one, so each awkward shape can be produced on purpose.

use std::fs;
use std::path::{Path, PathBuf};

use callerfilter_core::dataset::Dataset;
use callerfilter_datasets::{build_language, languages, upstream_version};

/// A throwaway source tree. Returns its root.
fn tree(files: &[(&str, &str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cf-convert-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&root);
    for (language, country, body) in files {
        let dir = root.join(language);
        fs::create_dir_all(&dir).expect("create language dir");
        fs::write(dir.join(format!("{country}.txt")), body).expect("write country file");
    }
    root
}

fn out_dir(root: &Path) -> PathBuf {
    let out = root.join("build");
    fs::create_dir_all(&out).expect("create out dir");
    out
}

fn read_back(out: &Path, language: &str) -> Dataset {
    let bytes = fs::read(out.join(format!("places.{language}.cfds"))).expect("read dataset");
    Dataset::parse(&bytes).expect("parse dataset")
}

const HEADER: &str = "# Copyright (C) 2011 The Libphonenumber Authors\n\
                      # Licensed under the Apache License, Version 2.0\n\n";

#[test]
fn licence_headers_and_blank_lines_are_skipped() {
    let root = tree(&[(
        "en",
        "39",
        &format!("{HEADER}391|Somewhere\n\n392|Elsewhere\n"),
    )]);
    let out = out_dir(&root);

    let report = build_language(&root, &out, "en", "9.0.33").expect("builds");

    assert_eq!(report.prefixes, 2);
    assert_eq!(report.countries, 1);
}

#[test]
fn a_file_with_only_a_licence_header_counts_as_no_country() {
    // Legitimate, not an error. It simply contributes nothing.
    let root = tree(&[
        ("en", "39", &format!("{HEADER}391|Somewhere\n")),
        ("en", "44", HEADER),
    ]);
    let out = out_dir(&root);

    let report = build_language(&root, &out, "en", "9.0.33").expect("builds");

    assert_eq!(report.prefixes, 1);
    assert_eq!(report.countries, 1, "the header-only file adds no country");
}

#[test]
fn a_language_covering_two_countries_is_normal() {
    // Italian covers exactly two in the real data. Sparse coverage is the rule,
    // not a failure.
    let root = tree(&[
        ("it", "39", &format!("{HEADER}391|Uno\n")),
        ("it", "41", &format!("{HEADER}411|Due\n")),
    ]);
    let out = out_dir(&root);

    let report = build_language(&root, &out, "it", "9.0.33").expect("builds");

    assert_eq!(report.countries, 2);
    assert_eq!(report.prefixes, 2);
}

#[test]
fn a_repeated_prefix_keeps_one_entry() {
    let root = tree(&[("en", "39", &format!("{HEADER}391|First\n391|Second\n"))]);
    let out = out_dir(&root);

    let report = build_language(&root, &out, "en", "9.0.33").expect("builds");

    assert_eq!(report.prefixes, 1);
    assert_eq!(read_back(&out, "en").lookup("3915"), Some("Second"));
}

#[test]
fn a_name_containing_the_delimiter_keeps_the_whole_name() {
    // Nothing in the vendored data does this today, so the converter must not
    // start silently truncating names if upstream ever ships one.
    let root = tree(&[("en", "39", &format!("{HEADER}391|North | South\n"))]);
    let out = out_dir(&root);

    build_language(&root, &out, "en", "9.0.33").expect("builds");

    assert_eq!(read_back(&out, "en").lookup("3915"), Some("North | South"));
}

#[test]
fn non_ascii_names_survive() {
    // Most of the non-English data is non-ASCII, so this is the normal case
    // rather than an exotic one.
    let root = tree(&[("ru", "7", &format!("{HEADER}7301|Республика Бурятия\n"))]);
    let out = out_dir(&root);

    build_language(&root, &out, "ru", "9.0.33").expect("builds");

    assert_eq!(
        read_back(&out, "ru").lookup("73011234"),
        Some("Республика Бурятия")
    );
}

#[test]
fn the_longest_prefix_wins_over_shorter_ones() {
    let root = tree(&[(
        "en",
        "39",
        &format!("{HEADER}391|Broad\n3912|Narrow\n391234567|Narrowest\n"),
    )]);
    let out = out_dir(&root);

    build_language(&root, &out, "en", "9.0.33").expect("builds");
    let d = read_back(&out, "en");

    assert_eq!(d.lookup("3919999"), Some("Broad"));
    assert_eq!(d.lookup("3912999"), Some("Narrow"));
    assert_eq!(d.lookup("391234567"), Some("Narrowest"));
}

#[test]
fn an_empty_language_directory_produces_an_empty_dataset() {
    let root = tree(&[("en", "39", &format!("{HEADER}391|Somewhere\n"))]);
    fs::create_dir_all(root.join("xx")).expect("create empty language");
    let out = out_dir(&root);

    let report = build_language(&root, &out, "xx", "9.0.33").expect("builds");

    assert_eq!(report.prefixes, 0);
    assert!(read_back(&out, "xx").is_empty());
}

#[test]
fn a_prefix_that_is_not_digits_fails_the_build() {
    // Better to stop than to ship a dataset that quietly lost a line.
    let root = tree(&[("en", "39", &format!("{HEADER}39a|Somewhere\n"))]);
    let out = out_dir(&root);

    let err = build_language(&root, &out, "en", "9.0.33").expect_err("must fail");

    assert!(err.contains("not digits"), "got {err}");
}

#[test]
fn a_line_with_no_delimiter_fails_the_build() {
    let root = tree(&[("en", "39", &format!("{HEADER}391 Somewhere\n"))]);
    let out = out_dir(&root);

    let err = build_language(&root, &out, "en", "9.0.33").expect_err("must fail");

    assert!(err.contains("prefix|name"), "got {err}");
}

#[test]
fn building_twice_produces_identical_bytes() {
    // The vendored-data check compares versions, not contents. If the build
    // were not reproducible, two people could ship different datasets from the
    // same source and nothing would notice.
    let root = tree(&[
        ("en", "39", &format!("{HEADER}392|Second\n391|First\n")),
        ("en", "44", &format!("{HEADER}441|Third\n")),
    ]);
    let out = out_dir(&root);

    build_language(&root, &out, "en", "9.0.33").expect("builds");
    let first = fs::read(out.join("places.en.cfds")).expect("read");
    build_language(&root, &out, "en", "9.0.33").expect("builds again");
    let second = fs::read(out.join("places.en.cfds")).expect("read");

    assert_eq!(first, second);
}

#[test]
fn the_dataset_records_which_release_it_came_from() {
    // So a reader can tell whether place names and number metadata agree.
    let root = tree(&[("en", "39", &format!("{HEADER}391|Somewhere\n"))]);
    let out = out_dir(&root);

    build_language(&root, &out, "en", "9.0.33").expect("builds");
    let d = read_back(&out, "en");

    assert_eq!(d.upstream(), "9.0.33");
    assert_eq!(d.language(), "en");
}

#[test]
fn asking_for_a_language_that_is_not_there_is_an_error() {
    let root = tree(&[("en", "39", &format!("{HEADER}391|Somewhere\n"))]);

    assert!(languages(&root, &[])
        .expect("lists")
        .contains(&"en".to_string()));
    assert!(languages(&root, &["zz".to_string()]).is_err());
}

#[test]
fn the_upstream_version_comes_from_the_provenance_file() {
    let root = tree(&[("en", "39", &format!("{HEADER}391|Somewhere\n"))]);
    let good = root.join("PROVENANCE.md");
    fs::write(&good, "| Upstream version | 9.0.33 |\n").expect("write");
    assert_eq!(upstream_version(&good).expect("reads"), "9.0.33");

    let bad = root.join("EMPTY.md");
    fs::write(&bad, "nothing here\n").expect("write");
    assert!(upstream_version(&bad).is_err());
}
