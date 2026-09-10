#!/bin/bash
# The vendored place names and the number metadata the core validates against
# must come from the same libphonenumber release.
#
# If they drift, the geocoder names prefixes the expander rejects, and a rule
# the user wrote against a city quietly stops matching. Nothing fails, nothing
# logs, and the rule looks fine on screen. That is why this is a build check and
# not a note in a file.
set -uo pipefail
cd "$(dirname "$0")/.."

UPSTREAM=data/upstream
fail=0

report() {
  fail=1
  echo "VENDORED DATA: $1"
  shift
  [ $# -gt 0 ] && printf '  %s\n' "$@"
}

# What the provenance says it is.
recorded=$(grep -E '^\| Upstream version \|' "$UPSTREAM/PROVENANCE.md" \
  | sed -E 's/.*\| ([0-9]+\.[0-9]+\.[0-9]+) \|.*/\1/')

if [ -z "$recorded" ]; then
  report "PROVENANCE.md does not record an upstream version." \
    "Expected a row: | Upstream version | X.Y.Z |"
fi

# What the crate actually compiles in. The build metadata after '+' is the
# libphonenumber release the crate carries.
locked=$(awk '/^name = "phonenumber"$/{getline; print}' Cargo.lock \
  | sed -E 's/version = "[^+]*\+([^"]*)"/\1/')

if [ -z "$locked" ]; then
  report "Could not read the phonenumber version from Cargo.lock."
elif [ -n "$recorded" ] && [ "$recorded" != "$locked" ]; then
  report "Place data and number metadata are from different releases." \
    "vendored under $UPSTREAM: $recorded" \
    "compiled into the core:    $locked" \
    "Re-copy the data and update PROVENANCE.md — see its Refreshing section."
fi

# A partial copy is worse than none: lookups just return nothing for the
# countries that went missing.
if [ ! -d "$UPSTREAM/geocoding" ]; then
  report "No geocoding data at $UPSTREAM/geocoding."
else
  languages=$(find "$UPSTREAM/geocoding" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')
  [ "$languages" -eq 0 ] && report "No languages under $UPSTREAM/geocoding."

  # English is the only near-complete language and everything falls back to it,
  # so an incomplete English tree breaks place names everywhere.
  english=$(find "$UPSTREAM/geocoding/en" -name '*.txt' 2>/dev/null | wc -l | tr -d ' ')
  if [ "$english" -lt 100 ]; then
    report "English geocoding looks truncated: $english country files." \
      "Everything falls back to English, so this breaks place names everywhere."
  fi
fi

[ -f "$UPSTREAM/PhoneNumberMetadata.xml" ] \
  || report "No number metadata at $UPSTREAM/PhoneNumberMetadata.xml."

# Redistribution without attribution is a licence breach, not an oversight.
[ -f NOTICE ] || report "NOTICE is missing, and this data is redistributed under Apache 2.0."

if [ $fail -ne 0 ]; then
  exit 1
fi

echo "Vendored data: libphonenumber $locked, $languages languages, $english English countries"
