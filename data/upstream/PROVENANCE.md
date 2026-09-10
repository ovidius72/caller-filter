# Where this data came from

Number metadata and place names, copied verbatim from Google's libphonenumber.

| | |
|---|---|
| Upstream project | [libphonenumber](https://github.com/google/libphonenumber) |
| Upstream version | 9.0.33 |
| Taken from | the `phonenumber` crate, version `0.3.10+9.0.33` |
| Copied on | 2026-09-10 |
| Licence | Apache License 2.0 — see NOTICE |

## Why it is copied from the crate and not fetched from upstream

The core validates numbers against the metadata compiled into the `phonenumber`
crate. Place names and number metadata have to describe the same world: if the
place data is newer, the geocoder names prefixes the expander rejects, and a
rule the user wrote against a city stops matching for no visible reason.

Upstream is usually ahead of the crate. Taking both halves from the crate is
what keeps them in step. `tools/check-vendored-data.sh` fails the build if they
drift.

## Why it is in the repository at all

The crate ships `assets/geocoding` but its `build.rs` compiles in only
`PhoneNumberMetadata.xml`. The geocoding files are present in the crate
directory and unreachable from our code, so they would not be in the app.

Reading them out of `~/.cargo` at build time would make builds depend on a
developer's machine. They are copied here instead.

## What is here

| Path | Contents |
|---|---|
| `geocoding/<lang>/<country code>.txt` | `<E.164 prefix>\|<place name>`, one per line, sorted |
| `PhoneNumberMetadata.xml` | possible lengths and validating patterns, per country per number type |

34 languages. Coverage is very uneven, and this shapes the product rather than
being a detail:

| Language | Countries | Prefix entries |
|---|---:|---:|
| English | 151 | 269,380 |
| Chinese | 2 | 130,209 |
| Portuguese | — | 12,237 |
| German | 5 | 6,545 |
| Italian | 2 | — |

English is the only near-complete language. Italian covers Italy and
Switzerland and nothing else. So R7's fallback — user's language, then English,
then the bare prefix — is the normal path for most users, not an edge case.

English alone is 269,380 prefix entries but only 38,116 distinct place names,
which is why the packaged format deduplicates names into a string table.

## Refreshing it

Bump the `phonenumber` dependency, then re-copy from the new crate version:

```sh
CRATE=$(find ~/.cargo/registry/src -maxdepth 2 -type d -name 'phonenumber-*' | sort -V | tail -1)
rm -rf data/upstream/geocoding
cp -R "$CRATE/assets/geocoding" data/upstream/geocoding
cp "$CRATE/assets/PhoneNumberMetadata.xml" data/upstream/
```

Then update `Upstream version` and `Copied on` above, and run
`tools/check-vendored-data.sh`.

A refresh can change what an existing rule matches — new lengths, a changed
pattern, a reassigned prefix. That is R5's problem, not this file's, but it is
the reason the rule is stored and never its expansion (Guidelines §2).
