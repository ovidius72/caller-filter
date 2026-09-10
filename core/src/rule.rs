//! The rule model: what a user authors, and what gets persisted.
//!
//! This module holds no evaluation logic. It defines the shapes the evaluator
//! consumes and the order it resolves them in.
//!
//! Two ideas carry most of the weight here.
//!
//! **Authored is not resolved.** A user can write seven kinds of rule, but
//! several of them mean the same match. A dialling prefix and a "starts with"
//! are both fixed leading digits, and a location resolves to a prefix through
//! the geocoder. So the authored form wraps the resolved form rather than
//! repeating it: five matchers exist, seven kinds are offered, and the
//! evaluator only ever sees the five.
//!
//! **Specificity is counted, not ranked by kind.** The more digits a rule
//! fixes, the more specific it is, the way a routing table prefers a longer
//! prefix. Nothing here knows what a country is, so nothing here can rank one
//! kind above another on anything but the digits it pins.

use std::cmp::Ordering;

/// Whether a rule blocks or permits.
///
/// Allow carves exceptions out of a deny. Where the platform evaluates live it
/// wins by being more specific; on iOS calls it is resolved at expansion time
/// by subtraction, because CallKit has no allow list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    Deny,
    Allow,
}

impl Effect {
    /// True when the two effects disagree, which is what makes a tie a conflict.
    fn opposes(self, other: Effect) -> bool {
        self != other
    }
}

/// A stable identifier for a rule.
///
/// The UI points at rules by id — "this rule is too broad for iPhone" names
/// one, a reported conflict names two — so the id has to survive editing and
/// reordering. Assigned by whatever stores the rule; the core never invents one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u64);

/// What went wrong building a rule out of user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// No digits at all after separators were removed.
    Empty,
    /// Something that is neither a digit nor a separator.
    NotADigit(char),
    /// A pattern with no fixed digit matches every number of its length. That
    /// is almost certainly not what the user meant, so it is refused here
    /// rather than silently blocking everything.
    PatternPinsNothing,
}

/// A validated run of digits, with separators removed.
///
/// Matching happens against the digits of an E.164 number without its leading
/// `+`. Nothing in this type knows a country code from a subscriber number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digits(String);

impl Digits {
    /// Accepts digits mixed with the separators people actually type. Rejects
    /// anything else rather than quietly dropping it, because a dropped
    /// character changes which numbers the rule matches.
    pub fn parse(raw: &str) -> Result<Self, RuleError> {
        let mut digits = String::with_capacity(raw.len());
        for c in raw.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else if !is_separator(c) {
                return Err(RuleError::NotADigit(c));
            }
        }
        if digits.is_empty() {
            return Err(RuleError::Empty);
        }
        Ok(Digits(digits))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Characters people put between digits, all of which carry no meaning.
fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | '-' | '/' | '(' | ')' | '+')
}

/// One or more leading-digit runs, any of which matches.
///
/// A single prefix is a set of one, so there is no separate code path for the
/// common case. The set exists because a place is not one prefix: "Turin" is
/// three area codes in the Italian data and "Guangzhou, Guangdong" is 1,565 in
/// the Chinese. Picking one of them would block part of a city and look like it
/// had worked, which is the silent narrowing R2 forbids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefixes {
    /// Sorted and deduplicated, never empty.
    runs: Vec<Digits>,
}

impl Prefixes {
    /// One prefix.
    pub fn one(prefix: Digits) -> Self {
        Prefixes { runs: vec![prefix] }
    }

    /// Several. Returns `None` when the set is empty, because a rule matching
    /// nothing is not a rule — a place that resolves to no prefix has to be
    /// reported, not quietly turned into a rule that never fires.
    ///
    /// Runs that sit inside another run are dropped. A number under `390111234`
    /// is already under `39011123`, so keeping both would change nothing about
    /// what matches while making the runs overlap — and overlapping runs
    /// produce the same number twice during expansion, which iOS rejects the
    /// whole request for. What is left is pairwise disjoint.
    pub fn many(mut runs: Vec<Digits>) -> Option<Self> {
        runs.sort();
        runs.dedup();

        // Sorted, so a run that contains another always comes first.
        let mut minimal: Vec<Digits> = Vec::with_capacity(runs.len());
        for run in runs {
            let covered = minimal
                .last()
                .is_some_and(|kept| run.as_str().starts_with(kept.as_str()));
            if !covered {
                minimal.push(run);
            }
        }

        if minimal.is_empty() {
            return None;
        }
        Some(Prefixes { runs: minimal })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Digits> {
        self.runs.iter()
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// The fewest digits any of them pins.
    ///
    /// Specificity is the broadest thing the rule matches, deliberately.
    /// A city's area codes differ in length — 39011 is five digits, 390122 is
    /// six — and the rule matches a number under any of them. Ranking it by the
    /// longest would claim a precision it does not have, and let it beat a rule
    /// that is genuinely narrower.
    fn shortest(&self) -> usize {
        self.runs.iter().map(|d| d.len()).min().unwrap_or(0)
    }

    /// True when any of these runs leads `digits`.
    ///
    /// Allocates nothing: the evaluator calls this on the call path.
    pub fn any_leads(&self, digits: &str) -> bool {
        self.runs.iter().any(|d| digits.starts_with(d.as_str()))
    }
}

/// One position in a pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atom {
    /// A digit that must appear exactly.
    Digit(u8),
    /// Any single digit.
    Any,
}

/// A positional match of digits and single-digit wildcards.
///
/// A pattern fixes the length as well as the digits it pins, because it has one
/// atom per position. `0987777xxx` matches ten-digit numbers only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    atoms: Vec<Atom>,
    pinned: u16,
}

impl Pattern {
    /// `x` marks a wildcard, in either case. Separators are dropped as
    /// elsewhere, so `0987.777.xxx` and `0987777xxx` are the same pattern.
    pub fn parse(raw: &str) -> Result<Self, RuleError> {
        let mut atoms = Vec::with_capacity(raw.len());
        let mut pinned: u16 = 0;
        for c in raw.chars() {
            if let Some(d) = c.to_digit(10) {
                atoms.push(Atom::Digit(d as u8));
                pinned = pinned.saturating_add(1);
            } else if c == 'x' || c == 'X' {
                atoms.push(Atom::Any);
            } else if !is_separator(c) {
                return Err(RuleError::NotADigit(c));
            }
        }
        if atoms.is_empty() {
            return Err(RuleError::Empty);
        }
        if pinned == 0 {
            return Err(RuleError::PatternPinsNothing);
        }
        Ok(Pattern { atoms, pinned })
    }

    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// The positions in order: `Some(digit)` where the pattern fixes one,
    /// `None` where any digit will do.
    ///
    /// Expansion generates numbers from these, so it needs the shape rather
    /// than only the yes/no of a match.
    pub fn slots(&self) -> impl Iterator<Item = Option<u8>> + '_ {
        self.atoms.iter().map(|a| match a {
            Atom::Digit(d) => Some(*d),
            Atom::Any => None,
        })
    }

    /// The leading digits the pattern fixes, stopping at the first wildcard.
    ///
    /// Which country a number belongs to is decided by its leading digits, so
    /// a pattern whose first positions vary belongs to no country in
    /// particular and cannot be expanded.
    pub fn fixed_prefix(&self) -> String {
        self.atoms
            .iter()
            .take_while(|a| matches!(a, Atom::Digit(_)))
            .map(|a| match a {
                Atom::Digit(d) => (b'0' + d) as char,
                Atom::Any => unreachable!("take_while stopped at the first wildcard"),
            })
            .collect()
    }

    /// True when this pattern could accept the given digit at that position.
    fn accepts_at(&self, index: usize, digit: u8) -> bool {
        match self.atoms.get(index) {
            Some(Atom::Digit(d)) => *d == digit,
            Some(Atom::Any) => true,
            None => false,
        }
    }

    /// True when every position is acceptable and the lengths agree.
    ///
    /// Whether a pattern accepts a run of digits is a property of the pattern,
    /// so it lives here. Which rule then wins is precedence, and that lives in
    /// `evaluate`. Takes `&str` because the evaluator borrows straight from the
    /// incoming number and must not allocate to ask this.
    pub fn matches_str(&self, digits: &str) -> bool {
        if digits.len() != self.atoms.len() {
            return false;
        }
        digits
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_digit() && self.accepts_at(i, b - b'0'))
    }

    fn matches_digits(&self, digits: &Digits) -> bool {
        self.matches_str(digits.as_str())
    }

    /// True when the two patterns could both accept some number.
    fn compatible_with(&self, other: &Pattern) -> bool {
        if self.atoms.len() != other.atoms.len() {
            return false;
        }
        self.atoms
            .iter()
            .zip(&other.atoms)
            .all(|(a, b)| match (a, b) {
                (Atom::Digit(x), Atom::Digit(y)) => x == y,
                _ => true,
            })
    }

    /// True when the pattern could accept a number whose leading digits are
    /// `lead`. A pattern shorter than the lead cannot.
    fn compatible_with_lead(&self, lead: &Digits) -> bool {
        if lead.len() > self.atoms.len() {
            return false;
        }
        lead.as_str()
            .bytes()
            .enumerate()
            .all(|(i, b)| self.accepts_at(i, b - b'0'))
    }

    /// The same from the other end.
    fn compatible_with_tail(&self, tail: &Digits) -> bool {
        if tail.len() > self.atoms.len() {
            return false;
        }
        let offset = self.atoms.len() - tail.len();
        tail.as_str()
            .bytes()
            .enumerate()
            .all(|(i, b)| self.accepts_at(offset + i, b - b'0'))
    }
}

/// A place the user picked.
///
/// Holds the name exactly as the data spells it, plus the language that
/// spelling came from — the same place is spelled differently in each language,
/// and a rule written in one must keep resolving if the user later switches the
/// app to another.
///
/// The core never invents one of these. The UI offers places the data actually
/// contains, so no name is ever typed or guessed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocationRef {
    pub name: String,
    pub language: String,
}

impl LocationRef {
    pub fn new(name: &str, language: &str) -> Self {
        LocationRef {
            name: name.to_string(),
            language: language.to_string(),
        }
    }
}

/// What the evaluator matches against. Five kinds, and location is not one of
/// them — it has already become a prefix by the time evaluation runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    /// Every digit fixed, and the length with them.
    Exact(Digits),
    /// Fixed leading digits, any tail. A dialling prefix resolves to this, and
    /// so does a place — which is why it is a set rather than one run.
    StartsWith(Prefixes),
    /// Fixed trailing digits, any head.
    EndsWith(Digits),
    /// Positional digits and wildcards.
    Pattern(Pattern),
    /// Matches the name the network offers, not the number.
    CallerId(String),
}

impl Matcher {
    /// How precisely this pins a number down.
    pub fn specificity(&self) -> Specificity {
        match self {
            Matcher::Exact(d) => Specificity::new(d.len(), true),
            Matcher::StartsWith(p) => Specificity::new(p.shortest(), false),
            Matcher::EndsWith(d) => Specificity::new(d.len(), false),
            Matcher::Pattern(p) => Specificity {
                pinned: p.pinned,
                fixes_length: true,
            },
            // A caller ID pins no digits. It is the least specific thing a user
            // can write, and deliberately ranks below a one-digit prefix.
            Matcher::CallerId(_) => Specificity::new(0, false),
        }
    }

    /// True unless the two matchers provably cannot both match one call.
    ///
    /// Deliberately conservative. Proving two rules disjoint often needs to
    /// know how long numbers are in a given country, and Guidelines §1 keeps
    /// that knowledge out of here. R2 says an equally specific conflict must be
    /// surfaced rather than resolved silently — and a conflict we failed to
    /// notice *is* a silent resolution. So when in doubt this says yes, and the
    /// user is asked about a pair that may turn out to be harmless.
    pub fn can_overlap(&self, other: &Matcher) -> bool {
        use Matcher::*;
        match (self, other) {
            // A caller ID and a number matcher look at different attributes of
            // the same call, so both can match at once.
            (CallerId(a), CallerId(b)) => a == b,
            (CallerId(_), _) | (_, CallerId(_)) => true,

            (Exact(a), Exact(b)) => a == b,
            (Exact(e), StartsWith(p)) | (StartsWith(p), Exact(e)) => p.any_leads(e.as_str()),
            (Exact(e), EndsWith(s)) | (EndsWith(s), Exact(e)) => e.as_str().ends_with(s.as_str()),
            (Exact(e), Pattern(p)) | (Pattern(p), Exact(e)) => p.matches_digits(e),

            // Any run of one containing any run of the other is enough: the two
            // rules then share at least one number.
            (StartsWith(a), StartsWith(b)) => a.iter().any(|x| {
                b.iter().any(|y| {
                    x.as_str().starts_with(y.as_str()) || y.as_str().starts_with(x.as_str())
                })
            }),
            (EndsWith(a), EndsWith(b)) => {
                a.as_str().ends_with(b.as_str()) || b.as_str().ends_with(a.as_str())
            }
            // A number can begin with one run and end with another. Ruling this
            // out needs the country's number length, which is data, not code.
            (StartsWith(_), EndsWith(_)) | (EndsWith(_), StartsWith(_)) => true,

            (Pattern(a), Pattern(b)) => a.compatible_with(b),
            (Pattern(p), StartsWith(s)) | (StartsWith(s), Pattern(p)) => {
                s.iter().any(|d| p.compatible_with_lead(d))
            }
            (Pattern(p), EndsWith(d)) | (EndsWith(d), Pattern(p)) => p.compatible_with_tail(d),
        }
    }
}

/// A rule as the user wrote it.
///
/// Seven kinds, wrapping the five the evaluator sees. `Prefix` and `Location`
/// both become `StartsWith`, so there is one implementation of that match and
/// the UI can still name the three separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authored {
    /// Written directly as one of the resolved matchers.
    Direct(Matcher),
    /// A dialling prefix. Matches as leading digits.
    Prefix(Digits),
    /// A city or region. Resolves to a prefix through the geocoder, then
    /// matches as leading digits.
    Location(LocationRef),
}

/// Supplies the prefixes behind a place. Implemented by the geocoder; declared
/// here so resolution has a boundary and the evaluator stays total.
///
/// Returns every prefix that names the place, not one. A place is routinely
/// several area codes, and returning the first would block part of a city while
/// looking like it had worked.
pub trait PrefixSource {
    fn prefixes_for(&self, location: &LocationRef) -> Vec<Digits>;
}

impl Authored {
    /// Turn an authored rule into the matcher the evaluator understands.
    ///
    /// Returns `None` when a location names no prefix in the current data. That
    /// is a real state, not an error: metadata changes, and a place that
    /// resolved last month may not today. The caller has to tell the user the
    /// rule stopped matching rather than pretend it still does.
    pub fn resolve(&self, places: &dyn PrefixSource) -> Option<Matcher> {
        match self {
            Authored::Direct(m) => Some(m.clone()),
            Authored::Prefix(d) => Some(Matcher::StartsWith(Prefixes::one(d.clone()))),
            Authored::Location(loc) => {
                Prefixes::many(places.prefixes_for(loc)).map(Matcher::StartsWith)
            }
        }
    }
}

/// How precisely a rule pins a number down.
///
/// Ordered by digits pinned first, then by whether the rule also fixes the
/// number's length — an exact number beats a prefix of the same length,
/// because the prefix also matches everything longer.
///
/// Two different rules may compare equal. That is legal, and it is what the
/// conflict report is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    pinned: u16,
    fixes_length: bool,
}

impl Specificity {
    fn new(pinned: usize, fixes_length: bool) -> Self {
        Specificity {
            // A rule long enough to overflow this is not a rule anyone wrote.
            pinned: u16::try_from(pinned).unwrap_or(u16::MAX),
            fixes_length,
        }
    }

    /// Digits the rule fixes.
    pub fn pinned(&self) -> u16 {
        self.pinned
    }

    /// Whether the rule also fixes how long the number is.
    pub fn fixes_length(&self) -> bool {
        self.fixes_length
    }
}

/// How a rule was written, where that changes what can be said about it.
///
/// A resolved location is indistinguishable from a hand-typed prefix once it
/// has resolved — they are the same match. But only one of them carries a
/// caveat the user has to be told: mobile numbering is not geographic anywhere,
/// so a rule written against a place can never match a mobile. Keeping the
/// origin is what lets `explain` say that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// Written as digits, a pattern, or a name.
    #[default]
    Direct,
    /// Written as a place, and resolved through the geocoder.
    Location,
}

/// A rule in the form the evaluator consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: RuleId,
    pub effect: Effect,
    pub matcher: Matcher,
    /// How the user wrote it. Does not affect matching.
    pub origin: Origin,
}

impl Rule {
    pub fn new(id: RuleId, effect: Effect, matcher: Matcher) -> Self {
        Rule {
            id,
            effect,
            matcher,
            origin: Origin::Direct,
        }
    }

    /// A rule the user wrote as a place.
    pub fn from_location(id: RuleId, effect: Effect, matcher: Matcher) -> Self {
        Rule {
            id,
            effect,
            matcher,
            origin: Origin::Location,
        }
    }

    pub fn specificity(&self) -> Specificity {
        self.matcher.specificity()
    }
}

/// Two rules that are equally specific, disagree, and could both match.
///
/// R2 requires these be shown to the user rather than settled by us. Nothing in
/// the core picks a winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub a: RuleId,
    pub b: RuleId,
}

/// The rules the evaluator runs against.
///
/// Held in descending specificity so the evaluator can walk from the most
/// specific and stop as soon as it drops below the best match it has found.
/// The ordering is built once here and never at evaluation time.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new(mut rules: Vec<Rule>) -> Self {
        // Descending specificity, with the id as a tiebreak so the order is
        // stable across runs. A stable order matters: the conflict report and
        // anything the UI shows must not reshuffle between launches.
        rules.sort_by(|a, b| match b.specificity().cmp(&a.specificity()) {
            Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        });
        RuleSet { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Rules from most specific to least. The evaluator's iteration order.
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    /// Every pair that is equally specific, disagrees, and could both match.
    ///
    /// Not on the call path. This runs when rules are edited, so the user can
    /// be asked; the evaluator must never build one of these per call.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut found = Vec::new();
        let mut start = 0;
        while start < self.rules.len() {
            // The set is sorted, so equal specificity is a contiguous run.
            let specificity = self.rules[start].specificity();
            let mut end = start + 1;
            while end < self.rules.len() && self.rules[end].specificity() == specificity {
                end += 1;
            }
            for i in start..end {
                for j in (i + 1)..end {
                    let (a, b) = (&self.rules[i], &self.rules[j]);
                    if a.effect.opposes(b.effect) && a.matcher.can_overlap(&b.matcher) {
                        found.push(Conflict { a: a.id, b: b.id });
                    }
                }
            }
            start = end;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Number-shaped fixtures live in core/tests/rule_model.rs, outside the
    // directory the Guidelines §1 lint scans. Digit runs are numbering data
    // wherever they appear in the shipped crate; test binaries are not shipped.

    #[test]
    fn separators_are_dropped_but_letters_are_refused() {
        assert_eq!(Digits::parse("1 2.3").unwrap().as_str(), "123");
        assert_eq!(Digits::parse("1a"), Err(RuleError::NotADigit('a')));
        assert_eq!(Digits::parse("..."), Err(RuleError::Empty));
    }

    #[test]
    fn a_pattern_must_pin_at_least_one_digit() {
        assert_eq!(Pattern::parse("xxx"), Err(RuleError::PatternPinsNothing));
        assert!(Pattern::parse("1xx").is_ok());
    }

    #[test]
    fn exact_beats_a_prefix_that_pins_the_same_digits() {
        let d = Digits::parse("123").unwrap();
        let exact = Matcher::Exact(d.clone()).specificity();
        let prefix = Matcher::StartsWith(Prefixes::one(d)).specificity();
        assert!(exact > prefix, "an exact number also fixes the length");
    }

    #[test]
    fn more_digits_beats_fewer() {
        let long = Matcher::StartsWith(Prefixes::one(Digits::parse("123").unwrap())).specificity();
        let short = Matcher::StartsWith(Prefixes::one(Digits::parse("12").unwrap())).specificity();
        assert!(long > short);
    }

    #[test]
    fn a_caller_id_is_the_least_specific_thing_a_user_can_write() {
        let name = Matcher::CallerId("anyone".into()).specificity();
        let one_digit =
            Matcher::StartsWith(Prefixes::one(Digits::parse("1").unwrap())).specificity();
        assert!(name < one_digit);
    }

    #[test]
    fn a_prefix_and_a_suffix_of_equal_length_tie() {
        let starts = Matcher::StartsWith(Prefixes::one(Digits::parse("12").unwrap())).specificity();
        let ends = Matcher::EndsWith(Digits::parse("34").unwrap()).specificity();
        assert_eq!(starts, ends, "neither pins more than the other");
    }

    #[test]
    fn an_empty_rule_set_has_nothing_to_report() {
        let empty = RuleSet::default();
        assert!(empty.is_empty());
        assert!(empty.conflicts().is_empty());
    }
}
