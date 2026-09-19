//! Build the versioned numbering metadata payload from vendored XML.
//! Runtime code never reads XML; this binary is a build-time conversion step.

use std::fs;
use std::io::BufReader;
use std::path::PathBuf;

use callerfilter_core::number_metadata::NumberMetadata;
use phonenumber::metadata::loader;

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = root.join("data/upstream/PhoneNumberMetadata.xml");
    let provenance = root.join("data/upstream/PROVENANCE.md");
    let output = root.join("datasets/build/number-metadata.cfnd");
    let provenance_text = fs::read_to_string(&provenance)
        .map_err(|e| format!("cannot read {}: {e}", provenance.display()))?;
    let upstream = provenance_text
        .lines()
        .find_map(|line| {
            line.strip_prefix("| Upstream version | ")
                .and_then(|v| v.strip_suffix(" |"))
        })
        .ok_or_else(|| "PROVENANCE.md has no upstream version row".to_string())?;
    let xml =
        fs::File::open(&source).map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    let metadata =
        loader::load(BufReader::new(xml)).map_err(|e| format!("cannot parse XML: {e:?}"))?;
    let bytes = NumberMetadata::build(upstream, metadata)
        .map_err(|e| format!("cannot frame metadata: {e:?}"))?;
    let parsed = NumberMetadata::parse(&bytes).map_err(|e| format!("round-trip failed: {e:?}"))?;
    if parsed.upstream() != upstream {
        return Err("upstream version did not survive round-trip".into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&output, bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    println!("wrote {}", output.display());
    Ok(())
}
