#!/bin/sh
# Writes the open-source libraries Kertas uses: the list of them in the
# About tab (src/builtin/acknowledgements.md), and the license notices
# their licenses ask to be included with Kertas, in the Licenses document
# (src/builtin/licenses.md). Run it after changing dependencies (the
# pre-commit hook in scripts/hooks runs it when Cargo.lock changes).
#
#   scripts/acknowledgements.sh           # write the file
#   scripts/acknowledgements.sh --check   # only check it lists exactly the
#                                         # libraries in Cargo.lock (offline)
#
# Needs cargo-about: `cargo install --locked cargo-about --features cli`.
# Writing the file needs the network: a few libraries don't include their
# license files, which cargo-about then fetches from their repositories.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out=src/builtin/acknowledgements.md
licenses=src/builtin/licenses.md

if ! command -v cargo-about >/dev/null 2>&1; then
    echo "acknowledgements.sh: cargo-about is missing; install it with:" >&2
    echo "    cargo install --locked cargo-about --features cli" >&2
    exit 1
fi

# The libraries in Kertas's About tab, as "name version" lines. Kertas itself
# isn't one of them.
libraries() {
    sed -n 's/^| \([^ |]*\) | \([^ |]*\) | .*/\1 \2/p' | grep -v '^kertas ' | sort
}

if [ "${1:-}" = "--check" ]; then
    # The list cargo-about makes, without fetching anything.
    expected=$(cargo about generate --frozen -c scripts/about.toml \
        scripts/acknowledgements.hbs 2>/dev/null | libraries)
    if [ "$expected" != "$(libraries < "$out")" ]; then
        echo "acknowledgements.sh: $out is out of date; run scripts/acknowledgements.sh" >&2
        exit 1
    fi
    exit 0
fi

# Errors are kept, but not the warning that Kertas itself has no license.
cargo about generate --locked -c scripts/about.toml scripts/acknowledgements.hbs \
    2> "$root/target/acknowledgements.log" |
    # Some license files have Windows line endings.
    tr -d '\r' |
    grep -v '^| kertas | ' > "$out.tmp" || {
    cat "$root/target/acknowledgements.log" >&2
    rm -f "$out.tmp"
    exit 1
}
# The template writes both files, separated by a line naming the second.
sed '/^@@@ licenses.md @@@$/,$d' "$out.tmp" > "$out"
sed '1,/^@@@ licenses.md @@@$/d' "$out.tmp" > "$licenses"
rm "$out.tmp"
echo "Wrote $out ($(libraries < "$out" | wc -l | tr -d ' ') libraries) and $licenses"
