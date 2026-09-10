//! What a rule will actually do, per platform.
//!
//! R2 requires that a rule never silently do nothing: its size and its verdict
//! on each platform have to be shown, computed from real data. This produces
//! those facts. It produces no words — wording is the shell's job, and R7 makes
//! UI text a data file rather than code.

use phonenumber::metadata::Database;

use crate::expand::{expand_matcher, Budget, Expansion, NotExpandable};
use crate::rule::{Effect, Matcher, Rule, RuleId, RuleSet};

/// A place rules are applied, described by what it can do rather than by name.
///
/// Held as data so that adding one — HarmonyOS is contemplated in F002 — is a
/// matter of describing it, not of editing a match arm here.
#[derive(Debug, Clone)]
pub struct Surface {
    /// Names this surface for the caller. Opaque here.
    pub id: String,
    /// True when rules run as a call or message arrives. False when the
    /// platform needs the numbers listed in advance, as iOS calls do.
    pub evaluates_live: bool,
    /// True when the surface can see the caller's name. iOS calls cannot.
    pub matches_caller_id: bool,
    /// How many numbers may be listed in advance. Only meaningful when
    /// `evaluates_live` is false. Comes from measurement (Guidelines §4).
    pub budget: Option<Budget>,
}

/// What a rule does on one surface.
///
/// These are five different situations for the person who wrote the rule, and
/// collapsing any two of them would lose exactly the honesty R2 asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Runs when a call or message arrives. Nothing is precomputed and no limit
    /// applies.
    AppliesLive,
    /// Listed in advance, and it fits.
    Fits { entries: u64 },
    /// Listed in advance, and there are more numbers than the platform holds.
    /// The figure is the real one, from the metadata.
    TooBroad { upper_bound: u64 },
    /// The platform cannot express this kind of rule at all.
    Inexpressible(NotExpandable),
    /// The rule is valid and changes nothing. An allow that no deny covers is
    /// the usual case: not being blocked is already the default.
    NoEffect,
}

/// A rule and what it does everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    pub rule: RuleId,
    /// One verdict per surface, in the order the surfaces were given.
    pub verdicts: Vec<(String, Verdict)>,
}

impl Explanation {
    /// The verdict for one surface, by id.
    pub fn on(&self, surface: &str) -> Option<&Verdict> {
        self.verdicts
            .iter()
            .find(|(id, _)| id == surface)
            .map(|(_, v)| v)
    }
}

/// Whether an allow rule carves anything out of anything.
///
/// An allow only means something inside a deny. On a live surface it wins by
/// being more specific; on iOS calls it removes numbers from the list. With no
/// deny over it there is nothing to carve, on any platform.
fn changes_anything(rule: &Rule, rules: &RuleSet) -> bool {
    if rule.effect == Effect::Deny {
        return true;
    }
    rules.iter().any(|other| {
        other.effect == Effect::Deny
            && other.id != rule.id
            && other.matcher.can_overlap(&rule.matcher)
    })
}

/// Work out what one rule does on each surface.
///
/// The rule set is needed because an allow's effect depends on what encloses
/// it, and the database because the size of an expansion is a fact about real
/// numbering data.
pub fn explain(
    rule: &Rule,
    rules: &RuleSet,
    database: &Database,
    surfaces: &[Surface],
) -> Explanation {
    let effective = changes_anything(rule, rules);

    let verdicts = surfaces
        .iter()
        .map(|surface| {
            let verdict = if !effective {
                Verdict::NoEffect
            } else if surface.evaluates_live {
                live_verdict(rule, surface)
            } else {
                listed_verdict(rule, surface, database)
            };
            (surface.id.clone(), verdict)
        })
        .collect();

    Explanation {
        rule: rule.id,
        verdicts,
    }
}

/// A surface that runs rules as calls arrive applies every kind of rule, unless
/// it cannot see what the rule matches on.
fn live_verdict(rule: &Rule, surface: &Surface) -> Verdict {
    match &rule.matcher {
        Matcher::CallerId(_) if !surface.matches_caller_id => {
            Verdict::Inexpressible(NotExpandable::CallerId)
        }
        _ => Verdict::AppliesLive,
    }
}

/// A surface that needs its numbers in advance can only take what expands.
fn listed_verdict(rule: &Rule, surface: &Surface, database: &Database) -> Verdict {
    // Without a budget nothing can be listed, so nothing applies.
    let Some(budget) = surface.budget else {
        return Verdict::Inexpressible(NotExpandable::NoLengths);
    };

    match expand_matcher(&rule.matcher, database, budget) {
        Expansion::Fits(numbers) => {
            let entries = numbers.count() as u64;
            if entries == 0 {
                // It expands, but to nothing that exists. The rule is real and
                // will never fire here, which the user has to be told.
                Verdict::NoEffect
            } else {
                Verdict::Fits { entries }
            }
        }
        Expansion::TooBroad { upper_bound } => Verdict::TooBroad { upper_bound },
        Expansion::NotExpandable(reason) => Verdict::Inexpressible(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_with_no_budget_can_list_nothing() {
        let surface = Surface {
            id: "nowhere".into(),
            evaluates_live: false,
            matches_caller_id: false,
            budget: None,
        };
        let rule = Rule::new(RuleId(1), Effect::Deny, Matcher::CallerId("anyone".into()));

        assert!(matches!(
            listed_verdict(&rule, &surface, &phonenumber::metadata::DATABASE),
            Verdict::Inexpressible(_)
        ));
    }
}
