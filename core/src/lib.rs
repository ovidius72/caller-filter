//! Shared call and SMS filtering engine.
//!
//! All filtering logic lives here and nowhere else. The iOS and Android shells
//! adapt this to an OS API and do no filtering of their own.
//!
//! Two constraints shape everything in this crate:
//!
//! - **Nothing country-specific in code.** No country, prefix, number length or
//!   place name may appear in a source file. All of it is data loaded at
//!   runtime, so adding a country is not a code change and not a release.
//!
//! - **The rule is the truth.** A user's rule is what gets stored. The expanded
//!   list of exact numbers that iOS needs is derived, and regenerated whenever
//!   the rule, the data, or the entry limit changes.
//!
//! This crate is pure logic with no platform dependency, so it is fully
//! testable on the host with no device or simulator.

uniffi::setup_scaffolding!();

pub mod dataset;
pub mod evaluate;
pub mod expand;
pub mod explain;
pub mod geocode;
pub mod normalize;
pub mod rule;

pub use dataset::{Dataset, DatasetError, Kind as DatasetKind};
pub use evaluate::{evaluate, Call, Verdict, E164};
pub use expand::{
    expand_matcher, expand_rules, expand_rules_to_vec, upper_bound, BlockList, Budget, Expansion,
    NotExpandable,
};
pub use explain::{explain, Caveat, Explanation, Surface, Verdict as PlatformVerdict};
pub use geocode::{geocode, is_geographic, Located, Places};
pub use normalize::{normalize, NormalizeError, Normalized};
pub use rule::{
    Authored, Conflict, Digits, Effect, LocationRef, Matcher, Origin, Pattern, PrefixSource,
    Prefixes, Rule, RuleError, RuleId, RuleSet, Specificity,
};

/// How the engine was asked to treat a call or message.
///
/// Android evaluates this live when a call arrives. iOS SMS evaluates it live
/// when a message arrives. iOS calls cannot evaluate anything at call time, so
/// there the decision is precomputed into a list of exact numbers instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block,
    /// Ring silently rather than reject. Android only; iOS offers no such control.
    Silence,
    /// Show text in place of the caller name. iOS calls only.
    Label(String),
}

/// The maximum number of entries the iOS Call Directory will accept.
///
/// Measured 2026-09-08 on iPhone 13 / iOS 26.6.1: the hard cap sits between
/// 1,900,000 and 2,000,000. Exceeding it returns
/// `CXErrorCodeCallDirectoryManagerErrorMaximumEntriesExceeded` (code 5).
///
/// This is a STARTING ESTIMATE, not a rule. Only one device was tested, and the
/// cap may differ by device or iOS version. The iOS shell must attempt the load
/// and back off when it sees error 5, so the app behaves correctly on hardware
/// nobody has measured. Never treat this as a constant to rely on, and never
/// expose it to the user — people do not think in entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryLimit(pub u32);

impl Default for EntryLimit {
    fn default() -> Self {
        // Near the measured cap rather than conservative: the only cost of a
        // larger list is a one-off background load, paid when the extension is
        // enabled or reloaded — never when a call arrives.
        EntryLimit(1_800_000)
    }
}

/// Placeholder so the crate builds and its test harness runs from day one.
/// The real surface (normalize / evaluate / geocode / expand / explain) arrives
/// in F002 once this scaffolding is proven end to end from Swift and Kotlin.
///
/// Exported across the FFI purely so both apps can prove they are talking to
/// this crate rather than to a stale copy.
///
/// Named `core_version` rather than `version` deliberately: the Swift bindings
/// land in the same module as the app code, and a bare `version` collides with
/// `NSObject.version` inside any class that inherits from it — which both iOS
/// extensions do.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The default entry limit, exposed so the iOS shell can seed its first attempt.
/// It must still back off when the device says the list is too long.
#[uniffi::export]
pub fn default_entry_limit() -> u32 {
    EntryLimit::default().0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert!(!core_version().is_empty());
    }

    #[test]
    fn default_entry_limit_is_under_the_measured_cap() {
        // The measured cap is between 1.9M and 2.0M. Anything at or above 1.9M
        // risks error 5 on the device it was measured on, let alone others.
        assert!(EntryLimit::default().0 < 1_900_000);
    }
}
