//! Turning rules into the list of exact numbers iOS needs.
//!
//! iOS runs no app code when a call arrives, so rules cannot be evaluated live
//! there. CallKit takes a list of exact numbers, prepared in advance. This is
//! the only shipped surface where a rule cannot run live, and it is where the
//! promise of "any rule you like" meets a hard limit.
//!
//! # What the validating pattern actually does
//!
//! Measured against the bundled metadata on 2026-09-10, because the plan
//! assumed otherwise and the assumption was wrong:
//!
//! - The pattern constrains **leading digits, not the tail**. 72 of 100
//!   two-digit German fixed-line prefixes are accepted; once one is accepted,
//!   every completion of every possible length is accepted too.
//! - So for Germany and Italy the pattern prunes **nothing**. German fixed-line
//!   numbers have eleven possible lengths (5 to 15), and libphonenumber really
//!   does permit all 10^13 numbers under a two-digit prefix.
//! - The tail does constrain somewhere: 37 of 165 territories with a viable
//!   two-digit fixed-line prefix showed a density below 1. But the worst
//!   observed was about 0.095, roughly a tenfold reduction. Never orders of
//!   magnitude.
//!
//! Two consequences shape this module. Applying the pattern is necessary to
//! avoid emitting numbers that do not exist — but it can never rescue a rule
//! from being too broad, because a tenfold cut to 10^13 is still 10^12. And
//! because the count follows from the possible lengths alone, the size of an
//! expansion is arithmetic, not something to be discovered by counting to
//! eleven trillion.

use phonenumber::metadata::{Database, Descriptor, Metadata};

use crate::rule::{Matcher, RuleSet};
use crate::{evaluate, Call, Decision, EntryLimit, E164};

/// Why a rule cannot become a list of numbers at all.
///
/// These are not failures. They are facts about the platform that the user has
/// to be told, because a rule that silently does nothing is the worst outcome
/// R2 allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotExpandable {
    /// iOS calls carry no caller name to match against. The rule works on
    /// Android and on iOS messages; on iOS calls it cannot apply at all.
    CallerId,
    /// Matching on trailing digits would mean walking a country's entire
    /// number space to find every number that ends a given way. Refused rather
    /// than attempted.
    Suffix,
    /// The leading digits belong to no country calling code in the data.
    UnknownCountry,
    /// The country has no number lengths for any type, so nothing can be
    /// generated. Metadata for a territory can be that sparse.
    NoLengths,
}

/// What a rule turns into on a platform that needs exact numbers.
#[derive(Debug)]
pub enum Expansion<'a> {
    /// It fits. Iterate for the numbers, ascending and without duplicates.
    Fits(Numbers<'a>),
    /// More numbers than the budget allows.
    ///
    /// `upper_bound` is the arithmetic ceiling from the possible lengths, which
    /// is the true count wherever the pattern does not constrain the tail. It
    /// is what the user should be shown: a real figure from real data.
    TooBroad { upper_bound: u64 },
    /// Not expressible as numbers.
    NotExpandable(NotExpandable),
}

/// How much work an expansion may do.
///
/// Both numbers are configuration measured elsewhere, never constants
/// (Guidelines §4). `max_entries` comes from the iOS measurement in F001.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// The most numbers the platform will accept.
    pub max_entries: u64,
    /// The most candidates to test before giving up on counting exactly.
    ///
    /// Only reached when the pattern rejects most of what it is offered. The
    /// worst density measured across the bundled metadata was about 0.095, so
    /// a candidate allowance of roughly ten times the entry budget resolves
    /// every territory seen. It is here so a pathological pattern cannot hang
    /// a background reload, not as a tuning knob.
    pub max_candidates: u64,
}

impl Budget {
    /// Derive a candidate allowance from the entry budget and the measured
    /// worst-case density.
    pub fn from_entry_limit(limit: EntryLimit, candidates_per_entry: u32) -> Self {
        let max_entries = u64::from(limit.0);
        Budget {
            max_entries,
            max_candidates: max_entries.saturating_mul(u64::from(candidates_per_entry)),
        }
    }
}

/// One position of a number: a digit that is fixed, or one that varies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Fixed(u8),
    Any,
}

/// A shape to generate numbers from: the national number, position by position.
///
/// A prefix rule becomes one template per possible length. A pattern rule
/// becomes exactly one. An exact number becomes one with nothing varying. That
/// is why there is only one generator below rather than one per matcher.
#[derive(Debug, Clone)]
struct Template {
    slots: Vec<Slot>,
}

impl Template {
    /// How many numbers this shape can produce, before the pattern is applied.
    fn candidates(&self) -> u64 {
        let varying = self.slots.iter().filter(|s| **s == Slot::Any).count();
        10u64.checked_pow(varying as u32).unwrap_or(u64::MAX)
    }
}

/// The numbers a rule expands to, produced one at a time.
///
/// Never builds the whole list. The iOS extension runs under tight memory
/// limits, and a list of a million numbers is not something to hold twice.
#[derive(Debug)]
pub struct Numbers<'a> {
    country_code: u16,
    templates: Vec<Template>,
    /// Which template we are on.
    template: usize,
    /// The varying digits of the current template, left to right. Incremented
    /// like an odometer, which walks the numbers in ascending order.
    odometer: Vec<u8>,
    /// False until the first `next`, so the first combination is not skipped.
    started: bool,
    descriptors: Vec<&'a Descriptor>,
    /// Reused so matching allocates nothing per candidate.
    buffer: String,
}

impl Numbers<'_> {
    /// Advance the odometer. Returns false when this template is exhausted.
    fn tick(&mut self) -> bool {
        for digit in self.odometer.iter_mut().rev() {
            if *digit < 9 {
                *digit += 1;
                return true;
            }
            *digit = 0;
        }
        false
    }

    /// Write the current combination into the buffer as a national number.
    fn render(&mut self) {
        self.buffer.clear();
        let mut varying = 0;
        for slot in &self.templates[self.template].slots {
            let d = match slot {
                Slot::Fixed(d) => *d,
                Slot::Any => {
                    let d = self.odometer[varying];
                    varying += 1;
                    d
                }
            };
            self.buffer.push((b'0' + d) as char);
        }
    }

    /// Move to the next template, or report that there are none left.
    fn next_template(&mut self) -> bool {
        self.template += 1;
        if self.template >= self.templates.len() {
            return false;
        }
        self.odometer = vec![
            0;
            self.templates[self.template]
                .slots
                .iter()
                .filter(|s| **s == Slot::Any)
                .count()
        ];
        true
    }
}

impl Iterator for Numbers<'_> {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        loop {
            if self.template >= self.templates.len() {
                return None;
            }
            if !self.started {
                self.started = true;
            } else if !self.tick() && !self.next_template() {
                return None;
            }

            self.render();
            // A number exists if any of the country's types accepts it. The
            // types are libphonenumber's taxonomy, and a territory defines only
            // some of them, so this is a union over whatever it defines.
            if self.descriptors.iter().any(|d| d.is_match(&self.buffer)) {
                if let Some(n) = to_int(self.country_code, &self.buffer) {
                    return Some(n);
                }
            }
        }
    }
}

/// Country code and national number as the single integer CallKit wants.
fn to_int(country_code: u16, national: &str) -> Option<i64> {
    let mut s = String::with_capacity(national.len() + 4);
    s.push_str(&country_code.to_string());
    s.push_str(national);
    s.parse::<i64>().ok()
}

/// Every descriptor a territory defines.
///
/// libphonenumber's type taxonomy is fixed and global; which of them a given
/// territory defines is not, which is why each is an Option. Short codes and
/// emergency numbers are left out deliberately — they are not dialable E.164
/// numbers and cannot appear as a caller.
fn descriptors_of(meta: &Metadata) -> Vec<&Descriptor> {
    let d = meta.descriptors();
    [
        d.fixed_line(),
        d.mobile(),
        d.toll_free(),
        d.premium_rate(),
        d.shared_cost(),
        d.personal_number(),
        d.voip(),
        d.pager(),
        d.uan(),
        d.voicemail(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Split E.164 digits into a country calling code and the rest.
///
/// Country codes are one to three digits and no code is a prefix of another,
/// so at most one split can be right. That is a property of E.164 itself, not
/// of any particular country.
fn split_country(digits: &str, database: &Database) -> Option<(u16, usize)> {
    for take in 1..=3usize {
        if digits.len() <= take {
            break;
        }
        let Ok(code) = digits[..take].parse::<u16>() else {
            continue;
        };
        if database.by_code(&code).is_some() {
            return Some((code, take));
        }
    }
    None
}

/// Expand one matcher into the numbers it covers.
///
/// The database is a parameter rather than the bundled global so that R5's
/// over-the-air metadata updates stay possible.
pub fn expand_matcher<'a>(
    matcher: &Matcher,
    database: &'a Database,
    budget: Budget,
) -> Expansion<'a> {
    // Only the leading fixed digits can say which country this is.
    let leading = match matcher {
        Matcher::CallerId(_) => return Expansion::NotExpandable(NotExpandable::CallerId),
        Matcher::EndsWith(_) => return Expansion::NotExpandable(NotExpandable::Suffix),
        Matcher::Exact(d) | Matcher::StartsWith(d) => d.as_str().to_string(),
        Matcher::Pattern(p) => p.fixed_prefix(),
    };

    let Some((code, code_len)) = split_country(&leading, database) else {
        return Expansion::NotExpandable(NotExpandable::UnknownCountry);
    };
    let national = &leading[code_len..];

    let metas = match database.by_code(&code) {
        Some(m) => m,
        None => return Expansion::NotExpandable(NotExpandable::UnknownCountry),
    };
    let descriptors: Vec<&Descriptor> = metas.iter().flat_map(|m| descriptors_of(m)).collect();
    if descriptors.is_empty() {
        return Expansion::NotExpandable(NotExpandable::NoLengths);
    }

    let templates = match matcher {
        // One number, nothing varies.
        Matcher::Exact(_) => vec![Template {
            slots: national.bytes().map(|b| Slot::Fixed(b - b'0')).collect(),
        }],
        // The pattern already says which positions vary. Drop the country code
        // from the front; what remains is the national number's shape.
        Matcher::Pattern(p) => vec![Template {
            slots: p
                .slots()
                .skip(code_len)
                .map(|s| match s {
                    Some(d) => Slot::Fixed(d),
                    None => Slot::Any,
                })
                .collect(),
        }],
        // A prefix means one shape per length the country allows, shortest
        // first — which is also ascending numerically, since every number here
        // shares a prefix and a longer one is always the larger.
        Matcher::StartsWith(_) => {
            let mut lengths: Vec<u16> = descriptors
                .iter()
                .flat_map(|d| d.possible_length().iter().copied())
                .filter(|l| usize::from(*l) > national.len())
                .collect();
            lengths.sort_unstable();
            lengths.dedup();
            lengths
                .into_iter()
                .map(|l| Template {
                    slots: national
                        .bytes()
                        .map(|b| Slot::Fixed(b - b'0'))
                        .chain(std::iter::repeat_n(
                            Slot::Any,
                            usize::from(l) - national.len(),
                        ))
                        .collect(),
                })
                .collect()
        }
        Matcher::CallerId(_) | Matcher::EndsWith(_) => unreachable!("returned above"),
    };

    if templates.is_empty() {
        return Expansion::NotExpandable(NotExpandable::NoLengths);
    }

    // The ceiling, before the pattern is applied. Where the pattern does not
    // constrain the tail — which is most places, including Germany and Italy —
    // this is the exact count. It is cheap, so it is worth asking first: no
    // amount of pattern pruning brings 10^13 under a budget of millions.
    let upper_bound = templates
        .iter()
        .fold(0u64, |acc, t| acc.saturating_add(t.candidates()));

    if upper_bound > budget.max_candidates {
        return Expansion::TooBroad { upper_bound };
    }

    let mut numbers = Numbers {
        country_code: code,
        odometer: vec![
            0;
            templates[0]
                .slots
                .iter()
                .filter(|s| **s == Slot::Any)
                .count()
        ],
        templates,
        template: 0,
        started: false,
        descriptors,
        buffer: String::with_capacity(24),
    };

    // The ceiling was affordable, so count for real. Only the pattern can bring
    // it below the ceiling, and only by a little.
    let mut counted = 0u64;
    while numbers.next().is_some() {
        counted += 1;
        if counted > budget.max_entries {
            return Expansion::TooBroad { upper_bound };
        }
    }

    // Rewind and hand the caller a fresh walk over the same shapes.
    numbers.template = 0;
    numbers.started = false;
    numbers.odometer = vec![
        0;
        numbers.templates[0]
            .slots
            .iter()
            .filter(|s| **s == Slot::Any)
            .count()
    ];
    Expansion::Fits(numbers)
}

/// Expand a whole rule set into the numbers a platform should block.
///
/// Allow rules are resolved here rather than at call time, because CallKit has
/// no allow list: a number the rules would permit is simply never emitted, so a
/// range of a thousand with one exception ships as nine hundred and ninety
/// nine. An allow rule that sits inside no deny expands to nothing at all,
/// since absence from the list already means allowed.
///
/// Which rule wins is not decided again here. Each candidate is put through
/// [`evaluate`], so the list iOS gets means exactly what Android does live.
/// There is one implementation of precedence and this is not it.
pub fn expand_rules<'a>(
    rules: &'a RuleSet,
    database: &'a Database,
    budget: Budget,
) -> BlockList<'a> {
    let streams = rules
        .iter()
        .filter(|rule| rule.effect == crate::Effect::Deny)
        .filter_map(
            |rule| match expand_matcher(&rule.matcher, database, budget) {
                Expansion::Fits(numbers) => Some(numbers),
                _ => None,
            },
        )
        .collect();

    BlockList {
        merge: Merge::new(streams),
        rules,
        last: None,
        text: String::with_capacity(24),
    }
}

/// Collect a whole block list into memory.
///
/// A convenience for tests and for callers that genuinely want the list at
/// once. The iOS extension must not use this: at the measured budget the list
/// is millions of numbers, and holding it is what this module exists to avoid.
pub fn expand_rules_to_vec(rules: &RuleSet, database: &Database, budget: Budget) -> Vec<i64> {
    expand_rules(rules, database, budget).collect()
}

/// A k-way merge over already-ascending streams.
///
/// Each rule's expansion arrives in order, so the merged order needs no buffer
/// and no sort — only a peeked head per stream. `k` is the number of deny
/// rules, which is small, so the heap costs nothing worth measuring.
#[derive(Debug)]
struct Merge<'a> {
    streams: Vec<Numbers<'a>>,
    /// The next value from each stream, or None once it is exhausted.
    heads: Vec<Option<i64>>,
}

impl<'a> Merge<'a> {
    fn new(mut streams: Vec<Numbers<'a>>) -> Self {
        let heads = streams.iter_mut().map(|s| s.next()).collect();
        Merge { streams, heads }
    }

    /// The smallest head, advancing the stream it came from.
    fn next(&mut self) -> Option<i64> {
        let mut best: Option<(usize, i64)> = None;
        for (i, head) in self.heads.iter().enumerate() {
            if let Some(v) = *head {
                if best.is_none_or(|(_, b)| v < b) {
                    best = Some((i, v));
                }
            }
        }
        let (i, v) = best?;
        self.heads[i] = self.streams[i].next();
        Some(v)
    }
}

/// The numbers a platform that needs exact numbers should block.
///
/// Ascending and free of duplicates, produced one at a time. Two rules can
/// cover the same number and iOS rejects the entire request if an entry repeats
/// or arrives out of order — but because the merge is ordered, dropping a
/// repeat is just skipping a value equal to the one before it. No set, no sort,
/// no list held in memory.
#[derive(Debug)]
pub struct BlockList<'a> {
    merge: Merge<'a>,
    rules: &'a RuleSet,
    /// The last value emitted, which is all deduplication needs.
    last: Option<i64>,
    /// Reused so the evaluate round trip does not allocate per number.
    text: String,
}

impl Iterator for BlockList<'_> {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        loop {
            let n = self.merge.next()?;
            if self.last == Some(n) {
                continue;
            }
            self.last = Some(n);

            // Rendered into the reused buffer through a stack array, so a list
            // of millions costs no allocations at all.
            self.text.clear();
            self.text.push('+');
            let mut digits = [0u8; 20];
            let mut at = digits.len();
            let mut left = n;
            while left > 0 {
                at -= 1;
                digits[at] = b'0' + (left % 10) as u8;
                left /= 10;
            }
            self.text
                .push_str(std::str::from_utf8(&digits[at..]).expect("digits are ASCII"));

            let Some(e164) = E164::new(&self.text) else {
                continue;
            };
            // Which rule wins is decided in one place, and this is not it. Every
            // candidate goes through the same evaluation Android runs live, so
            // the two surfaces cannot drift apart — and allow-exceptions fall
            // out of it rather than needing subtraction logic of their own.
            if evaluate(&Call::new(e164), self.rules).decision == Decision::Block {
                return Some(n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_counts_its_own_combinations() {
        let t = Template {
            slots: vec![Slot::Fixed(1), Slot::Any, Slot::Any],
        };
        assert_eq!(t.candidates(), 100);
    }

    #[test]
    fn an_odometer_with_no_varying_digits_produces_exactly_one() {
        let t = Template {
            slots: vec![Slot::Fixed(1), Slot::Fixed(2)],
        };
        assert_eq!(t.candidates(), 1);
    }

    #[test]
    fn a_budget_derives_its_candidate_allowance_from_the_entry_limit() {
        let b = Budget::from_entry_limit(EntryLimit(100), 7);
        assert_eq!(b.max_entries, 100);
        assert_eq!(b.max_candidates, 700);
    }
}
