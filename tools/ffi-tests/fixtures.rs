//! Shared fixtures for Rust and generated foreign binding tests. Never shipped.
#![allow(dead_code)]

use callerfilter_core::dataset::{Builder, Kind};
use callerfilter_core::number_metadata::NumberMetadata;
use phonenumber::metadata::loader::{self, Metadata};
use std::io::Cursor;

pub fn metadata() -> Vec<Metadata> {
    loader::load(Cursor::new(include_str!("numbering.xml"))).expect("synthetic metadata")
}

pub fn numbering() -> Vec<u8> {
    NumberMetadata::build("future-test-version", metadata()).expect("numbering")
}

pub fn places(language: &str, prefixes: &[&str]) -> Vec<u8> {
    let mut builder = Builder::new();
    for prefix in prefixes {
        builder.add(prefix, "Test Town");
    }
    builder
        .build(Kind::Places, language, "same-version")
        .expect("places")
}
