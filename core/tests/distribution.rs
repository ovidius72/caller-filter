//! How many digits a user must pin before a rule fits on the phone.
//!
//! R2 quotes figures for this. They were computed against a cap of 1,000,000
//! that turned out to be wrong, by a prototype that summed possible lengths
//! without applying the validating pattern. This recomputes them from the
//! metadata that ships today, against the cap actually measured on a device.
//!
//! Kept out of the default run because it walks every territory and takes a
//! few seconds. Run it with:
//!
//! ```text
//! cargo test --release --test distribution -- --ignored --nocapture
//! ```
//!
//! It prints a report rather than asserting a fixture. Metadata changes and R5
//! makes over-the-air updates a requirement, so a number pasted into a file
//! would rot exactly the way the one in R2 did.

use callerfilter_core::{upper_bound, Digits, EntryLimit, Matcher, NotExpandable, Prefixes};
use phonenumber::metadata::{Database, Descriptor, Metadata, DATABASE};

/// The bundled metadata, as a plain reference. Deref coercion from the lazy
/// static does not reach through every call site, so it is resolved once here.
fn db() -> &'static Database {
    &DATABASE
}

/// The measured iOS cap, injected rather than written here (Guidelines §4).
fn budget() -> u64 {
    u64::from(EntryLimit::default().0)
}

/// What we could work out about one territory and one kind of number.
enum Finding {
    /// The fewest national digits that have to be pinned for the expansion to
    /// fit, and the size at that point.
    Pinned { digits: usize, entries: u64 },
    /// No prefix of any length fits, so this kind of number cannot be filtered
    /// by range on a phone that needs a list.
    NeverFits,
    /// The metadata does not say enough to work it out.
    Unknown(&'static str),
}

/// Walk a real number from the metadata, one digit at a time, and find the
/// shortest prefix whose expansion fits.
///
/// Uses the descriptor's own example number so the prefix is one the validating
/// pattern actually accepts. A prefix invented here might be a shape no number
/// of that kind ever takes, which would make the answer meaningless.
fn shortest_fitting_prefix(country_code: u16, descriptor: &Descriptor) -> Finding {
    let Some(example) = descriptor.example() else {
        return Finding::Unknown("no example number");
    };
    if descriptor.possible_length().is_empty() {
        return Finding::Unknown("no possible lengths");
    }

    for pinned in 1..=example.len() {
        let prefix = format!("{country_code}{}", &example[..pinned]);
        let Ok(digits) = Digits::parse(&prefix) else {
            return Finding::Unknown("prefix would not parse");
        };

        match upper_bound(&Matcher::StartsWith(Prefixes::one(digits)), db()) {
            Ok(entries) if entries <= budget() => {
                return Finding::Pinned {
                    digits: pinned,
                    entries,
                }
            }
            Ok(_) => continue,
            Err(NotExpandable::UnknownCountry) => {
                return Finding::Unknown("country code not in the data")
            }
            Err(NotExpandable::NoLengths) => return Finding::Unknown("no lengths for any type"),
            Err(_) => return Finding::Unknown("not expandable"),
        }
    }
    Finding::NeverFits
}

/// Both kinds R2 quotes figures for.
fn kinds(meta: &Metadata) -> [(&'static str, Option<&Descriptor>); 2] {
    [
        ("mobile", meta.descriptors().mobile()),
        ("landline", meta.descriptors().fixed_line()),
    ]
}

#[test]
#[ignore = "walks every territory; run with --ignored --nocapture"]
fn recompute_the_digit_distribution() {
    let cap = budget();
    println!("\n=== digits that must be pinned for a range rule to fit ===");
    println!("cap in use: {cap} entries (from the device measurement in F001)");
    println!("method: shortest prefix of the metadata's own example number whose");
    println!("        expansion ceiling fits the cap. The ceiling is what the");
    println!("        possible lengths allow; the validating pattern chooses");
    println!("        which prefixes are real, and was measured not to reduce");
    println!("        the count for a viable prefix by more than about tenfold.");

    for (label, _) in kinds(DATABASE.iter().next().expect("a territory")) {
        let mut histogram: Vec<usize> = vec![0; 20];
        let mut never = Vec::new();
        let mut unknown: Vec<(String, &'static str)> = Vec::new();
        let mut worst: Vec<(String, usize, u64)> = Vec::new();
        let mut total = 0usize;

        for meta in DATABASE.iter() {
            total += 1;
            let descriptor = kinds(meta)
                .into_iter()
                .find(|(l, _)| *l == label)
                .and_then(|(_, d)| d);

            let Some(descriptor) = descriptor else {
                unknown.push((meta.id().to_string(), "territory defines no such type"));
                continue;
            };

            match shortest_fitting_prefix(meta.country_code(), descriptor) {
                Finding::Pinned { digits, entries } => {
                    if digits < histogram.len() {
                        histogram[digits] += 1;
                    }
                    worst.push((meta.id().to_string(), digits, entries));
                }
                Finding::NeverFits => never.push(meta.id().to_string()),
                Finding::Unknown(why) => unknown.push((meta.id().to_string(), why)),
            }
        }

        let resolved: usize = histogram.iter().sum();
        let within_four: usize = histogram.iter().take(5).sum();

        println!("\n--- {label} ---");
        println!("territories in the data: {total}");
        println!("  worked out:            {resolved}");
        println!("  never fits:            {}", never.len());
        println!("  not enough metadata:   {}", unknown.len());
        println!(
            "  need 4 digits or fewer: {within_four} of {resolved} worked out \
             ({} of all {total})",
            within_four
        );

        for (digits, count) in histogram.iter().enumerate() {
            if *count > 0 {
                println!("    {digits} digit(s): {count}");
            }
        }

        worst.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        println!("  hard tail (most digits needed):");
        for (region, digits, entries) in worst.iter().take(10) {
            println!("    {region:>4}  {digits:>2} digits  -> {entries} entries");
        }

        if !never.is_empty() {
            println!("  never fits: {}", never.join(" "));
        }
        if !unknown.is_empty() {
            let mut reasons: Vec<String> = unknown
                .iter()
                .take(12)
                .map(|(r, why)| format!("{r}({why})"))
                .collect();
            if unknown.len() > 12 {
                reasons.push(format!("and {} more", unknown.len() - 12));
            }
            println!("  not enough metadata: {}", reasons.join(" "));
        }
    }
    println!();
}

/// The ceiling must never promise more than the expander delivers.
///
/// The recomputation above trusts `upper_bound`. If that were optimistic
/// anywhere, every figure would be wrong in the dangerous direction — telling a
/// user a rule fits when the phone will reject it.
#[test]
fn the_ceiling_is_never_below_what_is_actually_generated() {
    use callerfilter_core::{expand_matcher, Budget, Expansion};

    let budget = Budget {
        max_entries: 100_000,
        max_candidates: 1_000_000,
    };

    for prefix in ["39021234567", "4930123456", "12025550", "44201234567"] {
        let Ok(digits) = Digits::parse(prefix) else {
            continue;
        };
        let matcher = Matcher::StartsWith(Prefixes::one(digits));

        let Ok(ceiling) = upper_bound(&matcher, db()) else {
            continue;
        };
        if let Expansion::Fits(numbers) = expand_matcher(&matcher, db(), budget) {
            let actual = numbers.count() as u64;
            assert!(
                actual <= ceiling,
                "{prefix}: generated {actual} but the ceiling promised {ceiling}"
            );
        }
    }
}
