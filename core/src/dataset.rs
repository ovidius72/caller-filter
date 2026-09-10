//! A versioned dataset: the container everything shipped as data lives in.
//!
//! Deliberately not "the geocoding format". R5 requires data to update without
//! an app release, R7 ships language files the same way, and R1's spam list
//! will ride the same pipe when a licence exists. One container, several kinds.
//!
//! # What it holds
//!
//! A dataset maps number prefixes to strings. Place names today; anything of
//! that shape later. The layout follows what the source data actually looks
//! like, measured 2026-09-10 across the vendored tree:
//!
//! - 269,380 English prefix entries, but only 38,116 distinct place names — so
//!   names go in a table once and entries reference them.
//! - The longest prefix anywhere is 9 digits, so a prefix fits in a `u32`.
//! - Entries arrive already sorted, with no duplicates.
//!
//! Entries are grouped by prefix length and sorted within a group, which is
//! what makes longest-match a handful of binary searches: try the longest
//! group first and stop at the first hit.
//!
//! # Where this runs
//!
//! In the app, not in an extension. The iOS Call Directory extension is handed
//! a finished list of numbers and never looks a place up; place names are for
//! writing a rule and for showing what a number is. P006's description implies
//! the extension's memory limit applies here — it does not.

use std::collections::BTreeMap;

/// Marks the file as ours and catches a truncated or foreign file immediately.
pub const MAGIC: &[u8; 4] = b"CFDS";

/// The layout below. Bump it when the layout changes, never for new content.
pub const FORMAT_VERSION: u16 = 1;

/// What a dataset is for. The container does not care; readers do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Kind {
    /// Number prefix to place name.
    Places = 1,
}

impl Kind {
    fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Kind::Places),
            _ => None,
        }
    }
}

/// What went wrong reading a dataset.
///
/// Every one of these means the data is unusable. A dataset that loads but is
/// wrong would make rules stop matching with nothing to explain it, so the
/// reader refuses rather than guesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetError {
    /// Not one of ours, or truncated at the very start.
    BadMagic,
    /// Written by a different layout than this code understands.
    UnsupportedFormat(u16),
    /// A dataset kind this build has no reader for.
    UnknownKind(u16),
    /// Ends earlier than its own header says it should.
    Truncated,
    /// A field that should be UTF-8 is not.
    NotUtf8,
    /// An entry points at a name that is not in the table.
    DanglingName { index: u32 },
    /// A prefix longer than the format stores. Refused rather than dropped: a
    /// missing prefix is a lookup that silently returns nothing.
    PrefixTooLong { prefix: String },
    /// Entries are out of order, so lookups would silently miss.
    Unsorted,
}

/// One prefix and the name it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The E.164 prefix, digits only, no leading `+`.
    pub prefix: String,
    pub name: String,
}

/// Build a dataset. Used by the converter; kept beside the reader so the layout
/// has exactly one definition.
#[derive(Debug, Default)]
pub struct Builder {
    /// Sorted so the output is byte-identical for identical input, whatever
    /// order entries were added in.
    entries: BTreeMap<String, String>,
}

impl Builder {
    pub fn new() -> Self {
        Builder::default()
    }

    /// Add one prefix. A repeated prefix keeps the last name given.
    pub fn add(&mut self, prefix: &str, name: &str) {
        self.entries.insert(prefix.to_string(), name.to_string());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialise.
    ///
    /// `upstream` records which release of the source data this came from, so a
    /// reader can tell whether place names and number metadata agree.
    ///
    /// Fails on a prefix too long to store rather than dropping it. The longest
    /// in the vendored data is nine digits, which is why a prefix is a `u32` —
    /// but a dropped entry would be a lookup that quietly returns nothing, and
    /// nothing downstream could tell that from a place that has no data.
    pub fn build(
        &self,
        kind: Kind,
        language: &str,
        upstream: &str,
    ) -> Result<Vec<u8>, DatasetError> {
        // Names first, deduplicated. This is the whole reason the format is
        // worth having: seven entries in ten share a name with another.
        let mut names: Vec<&str> = self.entries.values().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let index_of = |name: &str| {
            names
                .binary_search(&name)
                .expect("every name was collected above") as u32
        };

        // Group by prefix length, ascending within a group. Longest-match reads
        // the groups from the longest down.
        let mut by_length: BTreeMap<u8, Vec<(u32, u32)>> = BTreeMap::new();
        for (prefix, name) in &self.entries {
            let value = prefix
                .parse::<u32>()
                .map_err(|_| DatasetError::PrefixTooLong {
                    prefix: prefix.clone(),
                })?;
            by_length
                .entry(prefix.len() as u8)
                .or_default()
                .push((value, index_of(name)));
        }
        for group in by_length.values_mut() {
            group.sort_unstable();
        }

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(kind as u16).to_le_bytes());
        write_str(&mut out, language);
        write_str(&mut out, upstream);

        // Name table.
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in &names {
            write_str(&mut out, name);
        }

        // Entry groups.
        out.extend_from_slice(&(by_length.len() as u16).to_le_bytes());
        for (length, group) in &by_length {
            out.push(*length);
            out.extend_from_slice(&(group.len() as u32).to_le_bytes());
            for (value, name_index) in group {
                out.extend_from_slice(&value.to_le_bytes());
                out.extend_from_slice(&name_index.to_le_bytes());
            }
        }
        Ok(out)
    }
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// A dataset, parsed and checked.
#[derive(Debug)]
pub struct Dataset {
    kind: Kind,
    language: String,
    upstream: String,
    names: Vec<String>,
    /// Prefix length to its sorted `(prefix value, name index)` entries.
    groups: BTreeMap<u8, Vec<(u32, u32)>>,
}

impl Dataset {
    /// Parse and validate.
    ///
    /// Checks the ordering and every name reference, because a dataset that
    /// loads but is subtly wrong is worse than one that refuses: lookups would
    /// just quietly return nothing.
    pub fn parse(bytes: &[u8]) -> Result<Self, DatasetError> {
        let mut r = Cursor::new(bytes);

        if r.take(4)? != MAGIC {
            return Err(DatasetError::BadMagic);
        }
        let format = r.u16()?;
        if format != FORMAT_VERSION {
            return Err(DatasetError::UnsupportedFormat(format));
        }
        let kind_raw = r.u16()?;
        let kind = Kind::from_u16(kind_raw).ok_or(DatasetError::UnknownKind(kind_raw))?;
        let language = r.string()?;
        let upstream = r.string()?;

        let name_count = r.u32()?;
        let mut names = Vec::with_capacity(name_count as usize);
        for _ in 0..name_count {
            names.push(r.string()?);
        }

        let group_count = r.u16()?;
        let mut groups = BTreeMap::new();
        for _ in 0..group_count {
            let length = r.u8()?;
            let count = r.u32()?;
            let mut group = Vec::with_capacity(count as usize);
            let mut previous: Option<u32> = None;
            for _ in 0..count {
                let value = r.u32()?;
                let name_index = r.u32()?;
                if name_index as usize >= names.len() {
                    return Err(DatasetError::DanglingName { index: name_index });
                }
                if previous.is_some_and(|p| p >= value) {
                    return Err(DatasetError::Unsorted);
                }
                previous = Some(value);
                group.push((value, name_index));
            }
            groups.insert(length, group);
        }

        Ok(Dataset {
            kind,
            language,
            upstream,
            names,
            groups,
        })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Which language the names are in.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Which release of the source data this was built from.
    pub fn upstream(&self) -> &str {
        &self.upstream
    }

    pub fn len(&self) -> usize {
        self.groups.values().map(|g| g.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every prefix this dataset gives that exact name.
    ///
    /// The reverse of [`lookup`](Self::lookup), and it returns a set because a
    /// place is routinely several area codes. Scans the whole dataset: this
    /// runs when a rule is written, never when a call arrives.
    pub fn prefixes_named(&self, name: &str) -> Vec<String> {
        let Some(index) = self.names.iter().position(|n| n == name) else {
            return Vec::new();
        };
        let index = index as u32;

        let mut found = Vec::new();
        for (length, group) in &self.groups {
            for (value, name_index) in group {
                if *name_index == index {
                    // Restore the leading zeros the integer form dropped.
                    found.push(format!("{value:0width$}", width = usize::from(*length)));
                }
            }
        }
        found.sort();
        found
    }

    /// Every distinct place name, in order. What a picker offers.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    /// The name for the longest prefix of `digits` that this dataset knows.
    ///
    /// `digits` is an E.164 number without its `+`. Returns `None` when nothing
    /// matches, which is normal — coverage is very uneven, and a caller is
    /// expected to fall back to another language and then to showing nothing.
    pub fn lookup(&self, digits: &str) -> Option<&str> {
        for (length, group) in self.groups.iter().rev() {
            let take = *length as usize;
            if take > digits.len() {
                continue;
            }
            let Ok(value) = digits[..take].parse::<u32>() else {
                continue;
            };
            if let Ok(at) = group.binary_search_by_key(&value, |(v, _)| *v) {
                return Some(&self.names[group[at].1 as usize]);
            }
        }
        None
    }
}

/// Reads forward through the bytes, refusing to run off the end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DatasetError> {
        let end = self.at.checked_add(n).ok_or(DatasetError::Truncated)?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(DatasetError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, DatasetError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DatasetError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, DatasetError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<String, DatasetError> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        std::str::from_utf8(b)
            .map(str::to_string)
            .map_err(|_| DatasetError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Prefix-shaped fixtures live in core/tests/dataset.rs. The §1 lint refuses
    // digit runs here, and that strictness is worth keeping: these tests are
    // about the container, not about any country's numbering.

    #[test]
    fn a_foreign_file_is_refused() {
        assert_eq!(Dataset::parse(b"nope").unwrap_err(), DatasetError::BadMagic);
        assert_eq!(Dataset::parse(b"").unwrap_err(), DatasetError::Truncated);
    }

    #[test]
    fn an_empty_dataset_is_valid_and_finds_nothing() {
        let bytes = Builder::new()
            .build(Kind::Places, "it", "9.0.33")
            .expect("builds");
        let d = Dataset::parse(&bytes).expect("valid");
        assert!(d.is_empty());
        assert_eq!(d.lookup("391"), None);
    }

    #[test]
    fn building_is_reproducible_whatever_order_entries_arrive_in() {
        let mut forwards = Builder::new();
        forwards.add("391", "One");
        forwards.add("392", "Three");
        let mut backwards = Builder::new();
        backwards.add("392", "Three");
        backwards.add("391", "One");

        assert_eq!(
            forwards
                .build(Kind::Places, "en", "9.0.33")
                .expect("builds"),
            backwards
                .build(Kind::Places, "en", "9.0.33")
                .expect("builds")
        );
    }
}
