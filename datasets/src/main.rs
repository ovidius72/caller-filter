//! Builds the shipped datasets from the vendored source.
//!
//! With no arguments it builds every vendored language; name languages to build
//! a smaller app bundle. Which ones ship is a packaging decision, never a
//! constant — Guidelines §1 makes adding a language a matter of shipping a
//! file, and that is only true if nothing has to be edited to do it.

use std::fs;
use std::process::ExitCode;

use callerfilter_datasets::{
    build_language, languages, upstream_version, workspace_root, OUTPUT, PROVENANCE, SOURCE,
};

fn main() -> ExitCode {
    let root = workspace_root();
    let wanted: Vec<String> = std::env::args().skip(1).collect();

    let upstream = match upstream_version(&root.join(PROVENANCE)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cannot read the upstream version: {e}");
            eprintln!("  {PROVENANCE} must carry a row: | Upstream version | X.Y.Z |");
            return ExitCode::FAILURE;
        }
    };

    let source = root.join(SOURCE);
    let languages = match languages(&source, &wanted) {
        Ok(l) if l.is_empty() => {
            eprintln!("no languages to build under {}", source.display());
            return ExitCode::FAILURE;
        }
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot read {}: {e}", source.display());
            return ExitCode::FAILURE;
        }
    };

    let out_dir = root.join(OUTPUT);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("cannot create {}: {e}", out_dir.display());
        return ExitCode::FAILURE;
    }

    println!("libphonenumber {upstream}, {} language(s)", languages.len());
    let mut failed = false;

    for language in &languages {
        match build_language(&source, &out_dir, language, &upstream) {
            Ok(report) => println!(
                "  {:<8} {:>6} prefixes  {:>6} names  {:>4} countries  {:>7} bytes",
                report.language, report.prefixes, report.names, report.countries, report.bytes
            ),
            Err(e) => {
                eprintln!("  {language:<8} FAILED: {e}");
                failed = true;
            }
        }
    }

    if failed {
        eprintln!("\nsome datasets failed to build; nothing here should ship");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
