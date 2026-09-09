#!/bin/bash
# Enforces Project Guidelines §1 mechanically: no country, country code, prefix,
# number length, area code or place name may be compiled into the core.
#
# This is the project's central rule and the easiest one to break by accident —
# a "just this once" country check is how every hardcoded-metadata codebase
# starts. Review misses it; this does not.
#
# Everything country-specific comes from the datasets at runtime instead.
set -uo pipefail
cd "$(dirname "$0")/.."

SRC=core/src
fail=0

report() {
  fail=1
  echo "GUIDELINES §1 VIOLATION: $1"
  shift
  printf '  %s\n' "$@"
}

# Literal country codes / long digit runs. Allow small numbers and the measured
# entry limit, which is documented configuration rather than numbering data.
hits=$(grep -rnE '(^|[^A-Za-z0-9_])[0-9]{4,}' "$SRC" \
  | grep -vE 'EntryLimit|entry limit|1_800_000|1_900_000|2_000_000|//|^\s*//' \
  | grep -vE '#\[|test' || true)
[ -n "$hits" ] && report "long numeric literal — is this a phone prefix or length?" "$hits"

# Branching on a specific country.
hits=$(grep -rniE '(country|region)[_ ]?(code)?\s*==\s*"' "$SRC" || true)
[ -n "$hits" ] && report "code branches on a specific country" "$hits"

# ISO country codes in string literals or match arms.
hits=$(grep -rnE '"(IT|DE|FR|ES|GB|US|CH|AT|BR|CN|IN|MX)"' "$SRC" \
  | grep -vE '//|test' || true)
[ -n "$hits" ] && report "ISO country code literal in the core" "$hits"

# Place names.
hits=$(grep -rniE '"(milano|milan|roma|rome|berlin|london|paris|madrid|genova|torino)"' "$SRC" || true)
[ -n "$hits" ] && report "place name literal — these come from the geocoding data" "$hits"

if [ $fail -ne 0 ]; then
  cat <<'EOF'

Everything that varies by country is data loaded at runtime, so that adding a
country is neither a code change nor a release. If you genuinely need one of the
above, the rule is wrong and should be changed deliberately in the planner's
Project Guidelines — not worked around here.
EOF
  exit 1
fi

echo "Guidelines §1: no hardcoded numbering or place data in $SRC"
