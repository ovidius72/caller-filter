//! Versioned, runtime-loadable numbering metadata.
//!
//! XML is an input to build tooling only. Runtime consumers receive this
//! framed postcard payload and construct the phonenumber database atomically.
//!
//! CFNM v1: magic, little-endian u16 schema, u32 UTF-8 version length and
//! bytes, u32 payload length, then postcard `Vec<loader::Metadata>` from the
//! pinned phonenumber 0.3.10 DTO. Changing that DTO requires a schema bump;
//! the nonempty golden fixture in the tests guards dependency upgrades.
//! Content versions are opaque and may be newer than the engine. Loading is
//! eager, not lazy; callers must account for both old and new live snapshots.

use std::collections::{HashMap, HashSet};

use phonenumber::metadata::{loader, Database};

pub const MAGIC: &[u8; 4] = b"CFNM";
pub const FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberMetadataError {
    BadMagic,
    UnsupportedFormat(u16),
    Truncated,
    InvalidPayload,
    InvalidDatabase,
    UnsupportedRegion { region: String },
    TrailingBytes,
}

#[derive(Debug)]
pub struct NumberMetadata {
    upstream: String,
    database: Database,
}

impl NumberMetadata {
    pub fn build(
        upstream: &str,
        metadata: Vec<loader::Metadata>,
    ) -> Result<Vec<u8>, NumberMetadataError> {
        let payload =
            postcard::to_stdvec(&metadata).map_err(|_| NumberMetadataError::InvalidPayload)?;
        let mut out = Vec::with_capacity(4 + 2 + 4 + upstream.len() + 4 + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        write_string(&mut out, upstream)?;
        let payload_len =
            u32::try_from(payload.len()).map_err(|_| NumberMetadataError::InvalidPayload)?;
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&payload);
        Ok(out)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, NumberMetadataError> {
        let mut cursor = Cursor { bytes, at: 0 };
        if cursor.take(4)? != MAGIC {
            return Err(NumberMetadataError::BadMagic);
        }
        let version = cursor.u16()?;
        if version != FORMAT_VERSION {
            return Err(NumberMetadataError::UnsupportedFormat(version));
        }
        let upstream = cursor.string()?;
        let len = cursor.u32()? as usize;
        let payload = cursor.take(len)?;
        if cursor.at != bytes.len() {
            return Err(NumberMetadataError::TrailingBytes);
        }
        let (metadata, remainder): (Vec<loader::Metadata>, &[u8]) =
            postcard::take_from_bytes(payload).map_err(|_| NumberMetadataError::InvalidPayload)?;
        if !remainder.is_empty() {
            return Err(NumberMetadataError::InvalidPayload);
        }
        validate_metadata(&metadata)?;
        let database =
            Database::from(metadata).map_err(|_| NumberMetadataError::InvalidDatabase)?;
        Ok(NumberMetadata { upstream, database })
    }

    pub fn upstream(&self) -> &str {
        &self.upstream
    }
    pub fn database(&self) -> &Database {
        &self.database
    }
    pub fn into_parts(self) -> (String, Database) {
        (self.upstream, self.database)
    }
}

/// Check dependency preconditions before publishing a runtime database. In
/// particular Database::from unwraps patterns, and its shared-code validator
/// unwraps conversion of region strings to its compiled region enum.
fn validate_metadata(metadata: &[loader::Metadata]) -> Result<(), NumberMetadataError> {
    let mut keys = HashSet::new();
    let mut by_code: HashMap<u16, Vec<&str>> = HashMap::new();
    let mut id_codes: HashMap<&str, Vec<u16>> = HashMap::new();
    for meta in metadata {
        let id = meta
            .id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or(NumberMetadataError::InvalidDatabase)?;
        let code = meta
            .country_code
            .filter(|code| *code != 0)
            .ok_or(NumberMetadataError::InvalidDatabase)?;
        if !keys.insert((id, code)) || meta.general.is_none() {
            return Err(NumberMetadataError::InvalidDatabase);
        }
        by_code.entry(code).or_default().push(id);
        id_codes.entry(id).or_default().push(code);
        // This is the dependency's complete DTO, including non-E.164 types.
        // Any present descriptor is passed through its unchecked constructor.
        for desc in [
            &meta.general,
            &meta.fixed_line,
            &meta.mobile,
            &meta.toll_free,
            &meta.premium_rate,
            &meta.shared_cost,
            &meta.personal_number,
            &meta.voip,
            &meta.pager,
            &meta.uan,
            &meta.emergency,
            &meta.voicemail,
            &meta.short_code,
            &meta.standard_rate,
            &meta.carrier,
            &meta.no_international,
        ]
        .into_iter()
        .flatten()
        {
            if desc.national_number.as_deref().is_none_or(str::is_empty) {
                return Err(NumberMetadataError::InvalidDatabase);
            }
        }
    }
    for (id, codes) in &id_codes {
        // Repeated non-geographic IDs across distinct, unshared calling codes
        // are legal. Never reject those using a hard-coded special region ID.
        if codes.len() > 1
            && (id.parse::<phonenumber::country::Id>().is_ok()
                || codes.iter().any(|code| by_code[code].len() > 1))
        {
            return Err(NumberMetadataError::InvalidDatabase);
        }
        if codes.iter().any(|code| by_code[code].len() > 1)
            && id.parse::<phonenumber::country::Id>().is_err()
        {
            return Err(NumberMetadataError::UnsupportedRegion {
                region: (*id).into(),
            });
        }
    }
    Ok(())
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), NumberMetadataError> {
    let len = u32::try_from(value.len()).map_err(|_| NumberMetadataError::InvalidPayload)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], NumberMetadataError> {
        let end = self
            .at
            .checked_add(len)
            .ok_or(NumberMetadataError::Truncated)?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(NumberMetadataError::Truncated)?;
        self.at = end;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16, NumberMetadataError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, NumberMetadataError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn string(&mut self) -> Result<String, NumberMetadataError> {
        let len = self.u32()? as usize;
        let b = self.take(len)?;
        String::from_utf8(b.to_vec()).map_err(|_| NumberMetadataError::InvalidPayload)
    }
}
