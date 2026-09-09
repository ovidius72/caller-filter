//! Deciding what to do with one incoming call or message.
//!
//! This is the whole of the live decision, and per Guidelines §3 it is the only
//! place it exists. The shells hand in a call and act on the answer; they do no
//! filtering of their own.
//!
//! It runs on Android calls, Android SMS and iOS SMS. It does **not** run on
//! iOS calls: there, no app code executes when the phone rings, so the same
//! rules are pre-expanded into a list of exact numbers instead (P003). So this
//! function does not see every decision the product makes, and nothing here may
//! assume it does.
//!
//! Two limits shape the code. Android must answer within five seconds of being
//! asked and the phone does not ring until it does; the iOS SMS extension runs
//! under memory pressure. Neither is visible to the user, and both mean the
//! matching path allocates nothing and borrows everything.

use crate::rule::{Effect, Matcher, Rule, RuleSet, Specificity};
use crate::Decision;

/// A caller's number, already normalised.
///
/// Normalisation belongs with parsing in P003(F002), so this type does not do
/// it — it only refuses input that is not a normalised number at all. That
/// refusal is the point: a withheld caller has no number, and Android never
/// delivers one to us (PRESENTATION_RESTRICTED, UNKNOWN, UNAVAILABLE and
/// PAYPHONE calls never reach a screening service at all). There is deliberately
/// no placeholder for them, so no caller can invent `"unknown"` and have it
/// evaluated as though it were a number.
///
/// Borrows rather than owns, so building one costs nothing on the call path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct E164<'a>(&'a str);

impl<'a> E164<'a> {
    /// Accepts a normalised number, with or without its leading `+`. Returns
    /// `None` for anything else, including the empty string.
    ///
    /// It checks the shape and nothing more. How long a number should be varies
    /// by country and Guidelines §1 keeps that out of the core entirely.
    pub fn new(raw: &'a str) -> Option<Self> {
        let digits = raw.strip_prefix('+').unwrap_or(raw);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(E164(digits))
    }

    /// The digits, without the leading `+`. Rules match against these.
    pub fn digits(&self) -> &'a str {
        self.0
    }
}

/// One incoming call or message, as the rules see it.
///
/// The caller ID is separate from the number because a call carries both and a
/// rule may match either. It is absent more often than not.
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    number: E164<'a>,
    caller_id: Option<&'a str>,
}

impl<'a> Call<'a> {
    pub fn new(number: E164<'a>) -> Self {
        Call {
            number,
            caller_id: None,
        }
    }

    /// The name the network offered, when it offered one.
    pub fn with_caller_id(mut self, caller_id: &'a str) -> Self {
        self.caller_id = Some(caller_id);
        self
    }

    pub fn number(&self) -> E164<'a> {
        self.number
    }

    pub fn caller_id(&self) -> Option<&'a str> {
        self.caller_id
    }
}

/// What the engine decided, and what it had to work with.
///
/// The shells read `decision` and ignore the rest. The rest is for explaining
/// the decision back to the user, which R2 requires: a rule must never silently
/// start or stop applying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict<'a> {
    pub decision: Decision,
    /// The rule that decided, if any. Borrowed, so this costs no allocation.
    pub matched: Option<&'a Rule>,
    /// True when the best match tied with an opposing rule of equal
    /// specificity. See [`evaluate`] for what happens then.
    pub contested: bool,
}

/// Decide what to do with one call.
///
/// **Most specific wins.** The rule that pins the most digits decides,
/// whatever order the rules were written in — so an allow on an exact number
/// beats a deny on the range containing it, and the user never has to order
/// anything by hand. The rule set is held in descending specificity, so this
/// walks from the most specific and stops as soon as it drops below the best
/// match it has found.
///
/// **A tie is not resolved here.** When the best match ties with an opposing
/// rule of equal specificity, this does not pick a side. It allows the call and
/// sets `contested`, because the phone is ringing and something has to happen
/// now. Allowing is the recoverable half of that choice: an unwanted call that
/// gets through is visible and can be blocked afterwards, while a wanted call
/// that is blocked is silent — on iOS the system never even tells the app it
/// happened.
///
/// The real answer to a tie is to ask the user, and that happens at edit time
/// through [`RuleSet::conflicts`], not here. Building a conflict report costs a
/// pass over the rules and the call path cannot spend that on every call.
pub fn evaluate<'a>(call: &Call<'_>, rules: &'a RuleSet) -> Verdict<'a> {
    let mut winner: Option<&Rule> = None;
    let mut best: Option<Specificity> = None;
    let mut contested = false;

    for rule in rules.iter() {
        // Sorted descending, so once we are below the best match we have found,
        // nothing further can beat it.
        if let Some(best) = best {
            if rule.specificity() < best {
                break;
            }
        }

        if !matches(&rule.matcher, call) {
            continue;
        }

        match winner {
            None => {
                best = Some(rule.specificity());
                winner = Some(rule);
            }
            // Anything reaching here ties with the winner, or we would have
            // stopped above.
            Some(w) => {
                if w.effect != rule.effect {
                    contested = true;
                }
            }
        }
    }

    let decision = match winner {
        _ if contested => Decision::Allow,
        None => Decision::Allow,
        Some(w) => match w.effect {
            Effect::Allow => Decision::Allow,
            Effect::Deny => Decision::Block,
        },
    };

    Verdict {
        decision,
        matched: winner,
        contested,
    }
}

/// Whether one matcher accepts this call.
///
/// Every arm borrows. Nothing here allocates, compiles a regex, or looks at
/// anything but the call in hand.
fn matches(matcher: &Matcher, call: &Call<'_>) -> bool {
    let digits = call.number().digits();
    match matcher {
        Matcher::Exact(d) => digits == d.as_str(),
        Matcher::StartsWith(d) => digits.starts_with(d.as_str()),
        Matcher::EndsWith(d) => digits.ends_with(d.as_str()),
        Matcher::Pattern(p) => p.matches_str(digits),
        // Networks are not consistent about case, and a user typing a company
        // name should not have to match it. Compared without allocating.
        Matcher::CallerId(name) => call
            .caller_id()
            .is_some_and(|got| got.eq_ignore_ascii_case(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Number-shaped fixtures live in core/tests/, outside what the Guidelines
    // §1 lint scans. These cover only what needs no digits.

    #[test]
    fn a_number_is_accepted_with_or_without_its_plus() {
        assert_eq!(E164::new("+391").unwrap().digits(), "391");
        assert_eq!(E164::new("391").unwrap().digits(), "391");
    }

    #[test]
    fn a_withheld_caller_cannot_be_turned_into_a_number() {
        // Android never delivers one, and there is no placeholder to smuggle in.
        assert!(E164::new("").is_none());
        assert!(E164::new("+").is_none());
        assert!(E164::new("unknown").is_none());
        assert!(E164::new("private").is_none());
    }

    #[test]
    fn an_empty_rule_set_allows() {
        let number = E164::new("391").unwrap();
        let rules = RuleSet::default();
        let verdict = evaluate(&Call::new(number), &rules);

        assert_eq!(verdict.decision, Decision::Allow);
        assert_eq!(verdict.matched, None);
        assert!(!verdict.contested);
    }
}
