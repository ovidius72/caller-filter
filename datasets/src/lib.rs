//! Turns the vendored libphonenumber place data into shipped datasets.
//!
//! Run it with no arguments to build every vendored language:
//!
//! ```text
//! cargo run -p callerfilter-datasets --bin build-datasets
//! ```
//!
//! Or name the languages to build a smaller app bundle:
//!
//! ```text
//! cargo run -p callerfilter-datasets --bin build-datasets -- en it
//! ```
//!
//! Which languages ship is a packaging decision, made here or by whatever calls
//! this. It is deliberately not a constant anywhere: Guidelines §1 makes adding
//! a language a matter of shipping a file, and that is only true if nothing has
//! to be edited to do it.
//!
//! Everything it writes is validated by reading it back before it is accepted.
//! A silently truncated dataset makes rules stop matching with no error
//! anywhere, which is close to undebuggable on a user's phone.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use callerfilter_core::dataset::{Builder, Dataset, Kind};

/// Where the vendored source lives, relative to the workspace root.
pub const SOURCE: &str = "data/upstream/geocoding";
/// Where built datasets go. Build output, never committed.
pub const OUTPUT: &str = "datasets/build";
/// Records which libphonenumber release the source came from.
pub const PROVENANCE: &str = "data/upstream/PROVENANCE.md";

#[derive(Debug)]
pub struct Report {
    pub language: String,
    pub prefixes: usize,
    pub names: usize,
    pub countries: usize,
    pub bytes: usize,
}

/// Read one language's country files, pack them, and check the result parses
/// back to exactly what went in.
pub fn build_language(
    source: &Path,
    out_dir: &Path,
    language: &str,
    upstream: &str,
) -> Result<Report, String> {
    let dir = source.join(language);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    // Sorted so the output does not depend on how the filesystem enumerates.
    files.sort();

    let mut builder = Builder::new();
    // Kept for the read-back check: every prefix that went in must come out.
    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    let mut countries = 0usize;

    for file in &files {
        let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut had_entries = false;

        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            // Licence headers and blank lines. Every file starts with one.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // A name is allowed to contain anything except the delimiter, so
            // split once and keep the rest whole.
            let Some((prefix, name)) = line.split_once('|') else {
                return Err(format!(
                    "{}:{}: expected 'prefix|name'",
                    file.display(),
                    number + 1
                ));
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            if !prefix.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!(
                    "{}:{}: prefix {prefix:?} is not digits",
                    file.display(),
                    number + 1
                ));
            }
            builder.add(prefix, name);
            expected.insert(prefix.to_string(), name.to_string());
            had_entries = true;
        }

        // A file with only a licence header is legitimate; it just adds nothing.
        if had_entries {
            countries += 1;
        }
    }

    let bytes = builder
        .build(Kind::Places, language, upstream)
        .map_err(|e| format!("cannot pack: {e:?}"))?;

    // Read it back before accepting it. The reader checks ordering and every
    // name reference; this checks that nothing was lost on the way.
    let parsed =
        Dataset::parse(&bytes).map_err(|e| format!("built dataset will not parse: {e:?}"))?;
    if parsed.len() != expected.len() {
        return Err(format!(
            "packed {} prefixes but the source had {}",
            parsed.len(),
            expected.len()
        ));
    }
    if parsed.language() != language {
        return Err("language did not survive the round trip".to_string());
    }
    if parsed.upstream() != upstream {
        return Err("upstream version did not survive the round trip".to_string());
    }
    for (prefix, name) in &expected {
        match parsed.lookup(prefix) {
            Some(found) if found == name => {}
            Some(found) => {
                return Err(format!("{prefix} resolved to {found:?}, expected {name:?}"))
            }
            None => return Err(format!("{prefix} is missing from the packed dataset")),
        }
    }

    let names = count_names(&expected);
    let path = out_dir.join(format!("places.{language}.cfds"));
    fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;

    Ok(Report {
        language: language.to_string(),
        prefixes: parsed.len(),
        names,
        countries,
        bytes: bytes.len(),
    })
}

fn count_names(entries: &BTreeMap<String, String>) -> usize {
    let mut names: Vec<&str> = entries.values().map(|s| s.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names.len()
}

/// Every language directory, or only those asked for.
pub fn languages(source: &Path, wanted: &[String]) -> Result<Vec<String>, String> {
    let mut found: Vec<String> = fs::read_dir(source)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    found.sort();

    if wanted.is_empty() {
        return Ok(found);
    }
    for language in wanted {
        if !found.contains(language) {
            return Err(format!("no such language: {language}"));
        }
    }
    Ok(wanted.to_vec())
}

/// Pull the upstream release out of the provenance file, so the built dataset
/// carries the same version the vendored-data check enforces.
pub fn upstream_version(provenance: &Path) -> Result<String, String> {
    let text = fs::read_to_string(provenance).map_err(|e| e.to_string())?;
    text.lines()
        .find(|l| l.starts_with("| Upstream version |"))
        .and_then(|l| l.split('|').nth(2))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "no 'Upstream version' row".to_string())
}

/// The workspace root, found from this crate rather than the current directory,
/// so the tool works whatever it is run from.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits one level below the workspace root")
        .to_path_buf()
}
