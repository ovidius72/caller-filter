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
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert!(!version().is_empty());
    }

    #[test]
    fn default_entry_limit_is_under_the_measured_cap() {
        // The measured cap is between 1.9M and 2.0M. Anything at or above 1.9M
        // risks error 5 on the device it was measured on, let alone others.
        assert!(EntryLimit::default().0 < 1_900_000);
    }
}
