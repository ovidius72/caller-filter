//! Owned UniFFI boundary. Core matching remains in the Rust modules.
//!
//! Snapshots are immutable and Send + Sync. A failed constructor cannot replace
//! an old handle. Rules resolved from places retain that exact snapshot; direct
//! rules can be reused. Open datasets off the live evaluation path. Place files
//! are supplied in language preference order (requested language, then fallback).
//! No Rust lock is held while calling foreign code.

use std::collections::HashSet;
use std::sync::Arc;

use phonenumber::metadata::Database;

use crate::dataset::{Dataset, DatasetError};
use crate::expand::{expand_matcher, expand_rules, Budget, Expansion, NotExpandable};
use crate::explain::{self, Caveat, Surface};
use crate::geocode::{self, Located, Places};
use crate::normalize::{self, NormalizeError};
use crate::number_metadata::NumberMetadataError;
use crate::rule::{
    Digits, Effect, LocationRef, Matcher, Pattern, PrefixSource, Prefixes, Rule, RuleError, RuleId,
    RuleSet,
};
use crate::{evaluate, Call, Decision, E164};

#[derive(Debug, uniffi::Error)]
pub enum FfiError {
    InvalidNumber,
    LocationNeedsSnapshot,
    LocationUnresolved { name: String, language: String },
    Rule { reason: RuleFailure },
    Normalize { reason: NormalizeFailure },
    Dataset { reason: DataFailure },
    NumberMetadata { reason: DataFailure },
    SnapshotMismatch,
    DuplicateRuleId { id: u64 },
    UnknownRule { id: u64 },
    InvalidBatchSize,
    AllocationFailed,
    Callback { reason: String },
    UnexpectedCallback { reason: String },
}

/// Stable, machine-readable failures; platform UI supplies localized wording.
#[derive(Debug, uniffi::Enum)]
pub enum DataFailure {
    BadMagic,
    UnsupportedFormat { version: u16 },
    UnknownKind { kind: u16 },
    Truncated,
    NotUtf8,
    DanglingName { index: u32 },
    PrefixTooLong { prefix: String },
    Unsorted,
    DuplicateLength { length: u8 },
    CountTooLarge,
    TrailingBytes,
    InvalidPayload,
    InvalidDatabase,
    UnsupportedRegion { region: String },
}

#[derive(Debug, uniffi::Enum)]
pub enum NormalizeFailure {
    NotANumber,
    RegionRequired,
    UnknownRegion,
    RegionNotLoaded,
    NotValidForRegion,
}

#[derive(Debug, uniffi::Enum)]
pub enum RuleFailure {
    Empty,
    NotADigit { value: String },
    PatternPinsNothing,
}

#[derive(Debug, uniffi::Enum)]
pub enum EffectInput {
    Allow,
    Deny,
}

#[derive(Debug, uniffi::Enum)]
pub enum MatcherInput {
    Exact { digits: String },
    Prefix { digits: String },
    Suffix { digits: String },
    Pattern { value: String },
    CallerId { name: String },
    Location { name: String, language: String },
}

#[derive(Debug, uniffi::Record)]
pub struct RuleInput {
    pub id: u64,
    pub effect: EffectInput,
    pub matcher: MatcherInput,
}

#[derive(Debug, uniffi::Record)]
pub struct BudgetInput {
    pub max_entries: u64,
    pub max_candidates: u64,
}

#[derive(Debug, uniffi::Record)]
pub struct NormalizedOutput {
    pub e164: String,
}

#[derive(Debug, uniffi::Record)]
pub struct EvaluationOutput {
    pub decision: DecisionOutput,
    pub matched_rule: Option<u64>,
    pub contested: bool,
}

#[derive(Debug, uniffi::Enum)]
pub enum DecisionOutput {
    Allow,
    Block,
    Silence,
    Label { value: String },
}

#[derive(Debug, uniffi::Enum)]
pub enum LocatedOutput {
    Place { name: String, language: String },
    NotGeographic,
    NoData,
    NotANumber,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DatasetVersion {
    pub kind: u16,
    pub language: String,
    pub upstream: String,
    pub entries: u64,
}

#[derive(Debug, uniffi::Record)]
pub struct SurfaceInput {
    pub id: String,
    pub evaluates_live: bool,
    pub matches_caller_id: bool,
    pub budget: Option<BudgetInput>,
}

#[derive(Debug, uniffi::Enum)]
pub enum ExpansionOutput {
    Fits {
        entries: u64,
    },
    Cancelled {
        entries: u64,
    },
    TooBroad {
        upper_bound: u64,
        exact: bool,
        rule_ids: Vec<u64>,
    },
    NotExpandable {
        rule_id: u64,
        reason: NotExpandableOutput,
    },
}

#[derive(Debug, uniffi::Enum)]
pub enum NotExpandableOutput {
    CallerId,
    Suffix,
    UnknownCountry,
    NoLengths,
}

#[derive(Debug, uniffi::Enum)]
pub enum ExpansionStatus {
    Continue,
    Cancel,
}

fn not_expandable_output(reason: NotExpandable) -> NotExpandableOutput {
    match reason {
        NotExpandable::CallerId => NotExpandableOutput::CallerId,
        NotExpandable::Suffix => NotExpandableOutput::Suffix,
        NotExpandable::UnknownCountry => NotExpandableOutput::UnknownCountry,
        NotExpandable::NoLengths => NotExpandableOutput::NoLengths,
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum ExplainVerdict {
    AppliesLive,
    Fits { entries: u64 },
    TooBroad { upper_bound: u64 },
    Inexpressible { reason: NotExpandableOutput },
    NoEffect,
}

#[derive(Debug, uniffi::Enum)]
pub enum CaveatOutput {
    LandlinesOnly,
}

#[derive(Debug, uniffi::Record)]
pub struct SurfaceVerdict {
    pub surface_id: String,
    pub verdict: ExplainVerdict,
}

#[derive(Debug, uniffi::Record)]
pub struct ExplainOutput {
    pub rule: u64,
    pub verdicts: Vec<SurfaceVerdict>,
    pub caveats: Vec<CaveatOutput>,
}

#[derive(Debug, uniffi::Record)]
pub struct ConflictOutput {
    pub first: u64,
    pub second: u64,
}

/// Synchronous backpressure: each batch is released by Rust before continuing.
/// Return Cancel to stop after this batch, or throw a declared FfiError. An
/// unexpected foreign exception becomes UnexpectedCallback, never a Rust panic.
/// Foreign consumers must not retain all batches if they need bounded memory.
#[uniffi::export(foreign)]
pub trait ExpansionSink: Send + Sync {
    fn on_batch(&self, values: Vec<i64>) -> Result<ExpansionStatus, FfiError>;
}

struct SnapshotData {
    database: Database,
    places: Places,
    upstream: String,
    datasets: Vec<DatasetVersion>,
}

#[derive(uniffi::Object)]
pub struct Snapshot {
    data: Arc<SnapshotData>,
}

#[uniffi::export]
impl Snapshot {
    #[uniffi::constructor]
    pub fn new(numbering: Vec<u8>, places: Vec<Vec<u8>>) -> Result<Arc<Self>, FfiError> {
        let metadata =
            crate::number_metadata::NumberMetadata::parse(&numbering).map_err(FfiError::from)?;
        let (upstream, database) = metadata.into_parts();
        let mut loaded = Places::new();
        let mut versions = Vec::with_capacity(places.len());
        for bytes in places {
            let dataset = Dataset::parse(&bytes).map_err(FfiError::from)?;
            // Kind currently has only Places; the parser rejects unknown kinds.
            versions.push(DatasetVersion {
                kind: dataset.kind() as u16,
                language: dataset.language().to_string(),
                upstream: dataset.upstream().to_string(),
                entries: dataset.len() as u64,
            });
            loaded.push(dataset);
        }
        Ok(Arc::new(Self {
            data: Arc::new(SnapshotData {
                database,
                places: loaded,
                upstream,
                datasets: versions,
            }),
        }))
    }

    pub fn numbering_upstream(&self) -> String {
        self.data.upstream.clone()
    }
    pub fn dataset_versions(&self) -> Vec<DatasetVersion> {
        self.data.datasets.clone()
    }
    pub fn place_names(&self, language: String) -> Vec<String> {
        self.data
            .places
            .names_in(&language)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }
    pub fn place_prefixes(&self, name: String, language: String) -> Vec<String> {
        self.data
            .places
            .prefixes_for(&LocationRef::new(&name, &language))
            .into_iter()
            .map(|d| d.as_str().to_owned())
            .collect()
    }
    pub fn dataset_format_version(&self) -> u16 {
        crate::dataset::FORMAT_VERSION
    }
    pub fn number_metadata_format_version(&self) -> u16 {
        crate::number_metadata::FORMAT_VERSION
    }
}

#[derive(uniffi::Object)]
pub struct PreparedRules {
    rules: RuleSet,
    snapshot_data: Option<Arc<SnapshotData>>,
}

#[uniffi::export]
impl PreparedRules {
    #[uniffi::constructor]
    pub fn new(inputs: Vec<RuleInput>) -> Result<Arc<Self>, FfiError> {
        let mut ids = HashSet::with_capacity(inputs.len());
        let mut rules = Vec::with_capacity(inputs.len());
        for input in inputs {
            if !ids.insert(input.id) {
                return Err(FfiError::DuplicateRuleId { id: input.id });
            }
            let matcher = matcher_from_input(&input.matcher)?;
            let effect = match input.effect {
                EffectInput::Allow => Effect::Allow,
                EffectInput::Deny => Effect::Deny,
            };
            rules.push(Rule::new(RuleId(input.id), effect, matcher));
        }
        Ok(Arc::new(Self {
            rules: RuleSet::new(rules),
            snapshot_data: None,
        }))
    }

    pub fn len(&self) -> u64 {
        self.rules.len() as u64
    }
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
    pub fn conflicts(&self) -> Vec<ConflictOutput> {
        self.rules
            .conflicts()
            .into_iter()
            .map(|c| ConflictOutput {
                first: c.a.0,
                second: c.b.0,
            })
            .collect()
    }
}

#[uniffi::export]
pub fn prepare_rules_for_snapshot(
    inputs: Vec<RuleInput>,
    snapshot: &Snapshot,
) -> Result<Arc<PreparedRules>, FfiError> {
    let mut ids = HashSet::with_capacity(inputs.len());
    let mut rules = Vec::with_capacity(inputs.len());
    let mut derived = false;
    for input in inputs {
        if !ids.insert(input.id) {
            return Err(FfiError::DuplicateRuleId { id: input.id });
        }
        let is_location = matches!(&input.matcher, MatcherInput::Location { .. });
        derived |= is_location;
        let matcher = match &input.matcher {
            MatcherInput::Location { name, language } => {
                let location = LocationRef::new(name, language);
                let prefixes = snapshot.data.places.prefixes_for(&location);
                let prefixes =
                    Prefixes::many(prefixes).ok_or_else(|| FfiError::LocationUnresolved {
                        name: name.clone(),
                        language: language.clone(),
                    })?;
                Matcher::StartsWith(prefixes)
            }
            other => matcher_from_input(other)?,
        };
        let effect = match input.effect {
            EffectInput::Allow => Effect::Allow,
            EffectInput::Deny => Effect::Deny,
        };
        let rule = if is_location {
            Rule::from_location(RuleId(input.id), effect, matcher)
        } else {
            Rule::new(RuleId(input.id), effect, matcher)
        };
        rules.push(rule);
    }
    Ok(Arc::new(PreparedRules {
        rules: RuleSet::new(rules),
        snapshot_data: derived.then(|| Arc::clone(&snapshot.data)),
    }))
}

fn matcher_from_input(input: &MatcherInput) -> Result<Matcher, FfiError> {
    match input {
        MatcherInput::Exact { digits } => Ok(Matcher::Exact(
            Digits::parse(digits).map_err(FfiError::rule)?,
        )),
        MatcherInput::Prefix { digits } => Ok(Matcher::StartsWith(Prefixes::one(
            Digits::parse(digits).map_err(FfiError::rule)?,
        ))),
        MatcherInput::Suffix { digits } => Ok(Matcher::EndsWith(
            Digits::parse(digits).map_err(FfiError::rule)?,
        )),
        MatcherInput::Pattern { value } => Ok(Matcher::Pattern(
            Pattern::parse(value).map_err(FfiError::rule)?,
        )),
        MatcherInput::CallerId { name } => Ok(Matcher::CallerId(name.clone())),
        MatcherInput::Location { .. } => Err(FfiError::LocationNeedsSnapshot),
    }
}

#[uniffi::export]
pub fn normalize_number(
    raw: String,
    default_region: Option<String>,
    snapshot: &Snapshot,
) -> Result<NormalizedOutput, FfiError> {
    // The dependency unwraps a known hint region's database entry. A partial
    // runtime snapshot may not contain it. International input needs no hint.
    let hint = if raw.trim().starts_with('+') {
        None
    } else {
        default_region.as_deref()
    };
    if let Some(region) = hint.and_then(|r| r.parse::<phonenumber::country::Id>().ok()) {
        if snapshot.data.database.by_id(region.as_ref()).is_none() {
            return Err(FfiError::Normalize {
                reason: NormalizeFailure::RegionNotLoaded,
            });
        }
    }
    let normalized =
        normalize::normalize(&raw, hint, &snapshot.data.database).map_err(FfiError::normalize)?;
    Ok(NormalizedOutput {
        e164: normalized.as_e164().to_string(),
    })
}

#[uniffi::export]
pub fn evaluate_number(
    number: String,
    caller_id: Option<String>,
    rules: &PreparedRules,
) -> Result<EvaluationOutput, FfiError> {
    let e164 = E164::new(&number).ok_or(FfiError::InvalidNumber)?;
    let call = match caller_id.as_deref() {
        Some(id) => Call::new(e164).with_caller_id(id),
        None => Call::new(e164),
    };
    let verdict = evaluate(&call, &rules.rules);
    Ok(EvaluationOutput {
        decision: decision_output(verdict.decision),
        matched_rule: verdict.matched.map(|r| r.id.0),
        contested: verdict.contested,
    })
}

fn decision_output(decision: Decision) -> DecisionOutput {
    match decision {
        Decision::Allow => DecisionOutput::Allow,
        Decision::Block => DecisionOutput::Block,
        Decision::Silence => DecisionOutput::Silence,
        Decision::Label(value) => DecisionOutput::Label { value },
    }
}

#[uniffi::export]
pub fn geocode_number(number: String, snapshot: &Snapshot) -> LocatedOutput {
    match geocode::geocode(&number, &snapshot.data.places, &snapshot.data.database) {
        Located::Place { name, language } => LocatedOutput::Place { name, language },
        Located::NotGeographic => LocatedOutput::NotGeographic,
        Located::NoData => LocatedOutput::NoData,
        Located::NotANumber => LocatedOutput::NotANumber,
    }
}

#[uniffi::export]
pub fn is_number_geographic(number: String, snapshot: &Snapshot) -> bool {
    geocode::is_geographic(&number, &snapshot.data.database)
}

#[uniffi::export]
pub fn explain_rule(
    rule_id: u64,
    rules: &PreparedRules,
    snapshot: &Snapshot,
    surfaces: Vec<SurfaceInput>,
) -> Result<ExplainOutput, FfiError> {
    let rule = rules
        .rules
        .iter()
        .find(|r| r.id.0 == rule_id)
        .ok_or(FfiError::UnknownRule { id: rule_id })?;
    let surfaces: Vec<Surface> = surfaces
        .into_iter()
        .map(|s| Surface {
            id: s.id,
            evaluates_live: s.evaluates_live,
            matches_caller_id: s.matches_caller_id,
            budget: s.budget.map(|b| Budget {
                max_entries: b.max_entries,
                max_candidates: b.max_candidates,
            }),
        })
        .collect();
    check_snapshot(rules, snapshot)?;
    let explanation = explain::explain(rule, &rules.rules, &snapshot.data.database, &surfaces);
    let verdicts = explanation
        .verdicts
        .into_iter()
        .map(|(surface_id, verdict)| {
            let verdict = match verdict {
                explain::Verdict::AppliesLive => ExplainVerdict::AppliesLive,
                explain::Verdict::Fits { entries } => ExplainVerdict::Fits { entries },
                explain::Verdict::TooBroad { upper_bound } => {
                    ExplainVerdict::TooBroad { upper_bound }
                }
                explain::Verdict::Inexpressible(reason) => ExplainVerdict::Inexpressible {
                    reason: not_expandable_output(reason),
                },
                explain::Verdict::NoEffect => ExplainVerdict::NoEffect,
            };
            SurfaceVerdict {
                surface_id,
                verdict,
            }
        })
        .collect();
    let caveats = explanation
        .caveats
        .into_iter()
        .map(|c| match c {
            Caveat::LandlinesOnly => CaveatOutput::LandlinesOnly,
        })
        .collect();
    Ok(ExplainOutput {
        rule: explanation.rule.0,
        verdicts,
        caveats,
    })
}

/// Validate every deny, then count the merged result before the first callback.
/// max_entries caps the final list, after allow subtraction and deduplication.
/// max_candidates caps each deny's candidate space (not the entire rule set);
/// cost therefore scales with the number of denies and includes repeated walks.
/// TooBroad.exact distinguishes a complete merged count from a per-rule ceiling.
/// Cancellation/errors stop at a batch boundary; callbacks begin after preflight.
#[uniffi::export]
pub fn expand_rules_batched(
    rules: &PreparedRules,
    snapshot: &Snapshot,
    budget: BudgetInput,
    batch_size: u32,
    sink: Arc<dyn ExpansionSink>,
) -> Result<ExpansionOutput, FfiError> {
    check_snapshot(rules, snapshot)?;
    if batch_size == 0 {
        return Err(FfiError::InvalidBatchSize);
    }
    let budget = Budget {
        max_entries: budget.max_entries,
        max_candidates: budget.max_candidates,
    };
    // Per-rule expansion must be allowed to exceed the final merged cap: an
    // allow exception or overlap can make the merged stream fit.
    let stream_budget = Budget {
        max_entries: budget.max_entries.max(budget.max_candidates),
        max_candidates: budget.max_candidates,
    };
    for rule in rules.rules.iter().filter(|r| r.effect == Effect::Deny) {
        match expand_matcher(&rule.matcher, &snapshot.data.database, stream_budget) {
            Expansion::Fits(_) => {}
            Expansion::TooBroad { upper_bound } => {
                return Ok(ExpansionOutput::TooBroad {
                    upper_bound,
                    exact: false,
                    rule_ids: vec![rule.id.0],
                })
            }
            Expansion::NotExpandable(reason) => {
                return Ok(ExpansionOutput::NotExpandable {
                    rule_id: rule.id.0,
                    reason: not_expandable_output(reason),
                })
            }
        }
    }
    // Count the merged, deduplicated, allow-filtered stream first. This pass
    // guarantees no callback observes a partial list when the final cap fails.
    let total = expand_rules(&rules.rules, &snapshot.data.database, stream_budget).count() as u64;
    if total > budget.max_entries {
        return Ok(ExpansionOutput::TooBroad {
            upper_bound: total,
            exact: true,
            rule_ids: rules
                .rules
                .iter()
                .filter(|r| r.effect == Effect::Deny)
                .map(|r| r.id.0)
                .collect(),
        });
    }
    let stream = expand_rules(&rules.rules, &snapshot.data.database, stream_budget);
    let capacity = u64::from(batch_size).min(total) as usize;
    let mut batch = Vec::new();
    let mut emitted = 0u64;
    for value in stream {
        if batch.capacity() == 0 {
            batch
                .try_reserve_exact(capacity)
                .map_err(|_| FfiError::AllocationFailed)?;
        }
        batch.push(value);
        emitted += 1;
        if batch.len() == batch_size as usize {
            let values = std::mem::take(&mut batch);
            match sink.on_batch(values)? {
                ExpansionStatus::Continue => {}
                ExpansionStatus::Cancel => {
                    return Ok(ExpansionOutput::Cancelled { entries: emitted })
                }
            }
        }
    }
    if !batch.is_empty() {
        let values = batch;
        match sink.on_batch(values)? {
            ExpansionStatus::Continue => {}
            ExpansionStatus::Cancel => return Ok(ExpansionOutput::Cancelled { entries: emitted }),
        }
    }
    Ok(ExpansionOutput::Fits { entries: emitted })
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[uniffi::export]
pub fn default_entry_limit() -> u32 {
    crate::EntryLimit::default().0
}

impl FfiError {
    fn rule(error: RuleError) -> Self {
        let reason = match error {
            RuleError::Empty => RuleFailure::Empty,
            RuleError::NotADigit(value) => RuleFailure::NotADigit {
                value: value.to_string(),
            },
            RuleError::PatternPinsNothing => RuleFailure::PatternPinsNothing,
        };
        FfiError::Rule { reason }
    }
    fn normalize(error: NormalizeError) -> Self {
        let reason = match error {
            NormalizeError::NotANumber => NormalizeFailure::NotANumber,
            NormalizeError::RegionRequired => NormalizeFailure::RegionRequired,
            NormalizeError::UnknownRegion => NormalizeFailure::UnknownRegion,
            NormalizeError::NotValidForRegion => NormalizeFailure::NotValidForRegion,
        };
        FfiError::Normalize { reason }
    }
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FfiError {}

fn check_snapshot(rules: &PreparedRules, snapshot: &Snapshot) -> Result<(), FfiError> {
    if rules
        .snapshot_data
        .as_ref()
        .is_some_and(|expected| !Arc::ptr_eq(expected, &snapshot.data))
    {
        return Err(FfiError::SnapshotMismatch);
    }
    Ok(())
}

impl From<uniffi::UnexpectedUniFFICallbackError> for FfiError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::UnexpectedCallback {
            reason: error.reason,
        }
    }
}

impl From<NumberMetadataError> for FfiError {
    fn from(error: NumberMetadataError) -> Self {
        let reason = match error {
            NumberMetadataError::BadMagic => DataFailure::BadMagic,
            NumberMetadataError::UnsupportedFormat(version) => {
                DataFailure::UnsupportedFormat { version }
            }
            NumberMetadataError::Truncated => DataFailure::Truncated,
            NumberMetadataError::InvalidPayload => DataFailure::InvalidPayload,
            NumberMetadataError::InvalidDatabase => DataFailure::InvalidDatabase,
            NumberMetadataError::UnsupportedRegion { region } => {
                DataFailure::UnsupportedRegion { region }
            }
            NumberMetadataError::TrailingBytes => DataFailure::TrailingBytes,
        };
        Self::NumberMetadata { reason }
    }
}

impl From<DatasetError> for FfiError {
    fn from(error: DatasetError) -> Self {
        let reason = match error {
            DatasetError::BadMagic => DataFailure::BadMagic,
            DatasetError::UnsupportedFormat(version) => DataFailure::UnsupportedFormat { version },
            DatasetError::UnknownKind(kind) => DataFailure::UnknownKind { kind },
            DatasetError::Truncated => DataFailure::Truncated,
            DatasetError::NotUtf8 => DataFailure::NotUtf8,
            DatasetError::DanglingName { index } => DataFailure::DanglingName { index },
            DatasetError::PrefixTooLong { prefix } => DataFailure::PrefixTooLong { prefix },
            DatasetError::Unsorted => DataFailure::Unsorted,
            DatasetError::DuplicateLength { length } => DataFailure::DuplicateLength { length },
            DatasetError::CountTooLarge => DataFailure::CountTooLarge,
            DatasetError::TrailingBytes => DataFailure::TrailingBytes,
        };
        Self::Dataset { reason }
    }
}
