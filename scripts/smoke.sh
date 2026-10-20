#!/bin/sh
# Runs every smoke test mode (src/smoke.rs) against a debug build, each on
# a scratch copy of a fixture with its own empty settings folder, and stops
# at the first failure.
#
#   scripts/smoke.sh
#
# The smoke tests open Kertas windows and take keyboard focus for a minute
# or two, so leave the keyboard alone while they run. The typing benchmark
# is scripts/bench.sh.
set -eu

suite_started=$(date +%s)
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo build --quiet
kertas="${CARGO_TARGET_DIR:-$root/target}/debug/kertas"
fixtures="$root/tests/fixtures"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Runs one mode on a copy of a fixture, with a time limit. Extra
# environment variables can follow the fixture name. Prints the output and
# how long the mode took, and succeeds only if it says "SMOKE OK". The
# output is printed with printf, as /bin/sh's echo turns a check's `\n`
# into a line break.
run() {
    mode=$1
    fixture=$2
    shift 2
    dir="$work/$mode"
    mkdir -p "$dir/config"
    cp "$fixtures/$fixture" "$dir/$fixture"
    cp -R "$fixtures/images" "$dir/images"
    echo "==> $mode ($fixture)"
    started=$(date +%s)
    # macOS has no timeout(1); perl's alarm kills Kertas if it hangs.
    if ! output=$(env KERTAS_SMOKE="$mode" KERTAS_CONFIG_DIR="$dir/config" "$@" \
        perl -e 'alarm shift; exec @ARGV' 300 "$kertas" "$dir/$fixture" 2>&1); then
        printf '%s\n' "$output" | tail -20
        echo "smoke.sh: FAILED: $mode exited with an error" >&2
        exit 1
    fi
    printf '%s\n' "$output" | grep -E "^SMOKE|: (ok|FAILED)|p50" | sed 's/^/    /'
    echo "    ($(($(date +%s) - started)) s)"
    if ! printf '%s\n' "$output" | grep -q "^SMOKE OK"; then
        echo "smoke.sh: FAILED: $mode" >&2
        exit 1
    fi
}

run 1 10k.md

# write: the text Enter, lists, quotes and indentation produce.
echo "==> write"
mkdir -p "$work/write/config"
printf 'Para' > "$work/write/write.md"
KERTAS_SMOKE=write KERTAS_CONFIG_DIR="$work/write/config" \
    perl -e 'alarm shift; exec @ARGV' 120 "$kertas" "$work/write/write.md" >/dev/null 2>&1
expected=$(printf 'Para\n\n\nNew paragraph\\\nsecond line\n\n- item\n  - [ ] task\n- [ ] sub\n\n> quote\n\n\n\n```\nfn\t\n        x\n    y')
if [ "$(cat "$work/write/write.md")" != "$expected" ]; then
    echo "smoke.sh: FAILED: write produced different text:" >&2
    cat "$work/write/write.md" >&2
    exit 1
fi
echo "    SMOKE OK (text as expected)"

run tabs showcase.md
mkdir -p "$work/windows/config"
echo 'layout = "windows"' > "$work/windows/config/config.toml"
run windows showcase.md
run settings showcase.md
run settings-window showcase.md
run help showcase.md
run reveal showcase.md
run clicks clicks.md
run anchors anchors.md
run footnotes footnotes.md
run reading anchors.md
run scrollbar 10k.md
run scrollbar-drag showcase.md
run images showcase.md
run a11y a11y.md
run print showcase.md KERTAS_PRINT_PDF="$work/print/out.pdf"
run spelling spelling.md

# convert: kt --convert lays out a fixture as Print does (the same number
# of pages as the print mode's PDF), without a window, and rejects bad
# arguments.
echo "==> convert"
mkdir -p "$work/convert/config"
cp "$fixtures/showcase.md" "$work/convert/showcase.md"
cp -R "$fixtures/images" "$work/convert/images"
convert() {
    (cd "$work/convert" && KERTAS_CONFIG_DIR="$work/convert/config" \
        perl -e 'alarm shift; exec @ARGV' 60 "$kertas" --convert "$@" 2>&1)
}
# The page tree's /Count is the number of pages.
pages() { LC_ALL=C grep -ao '/Count [0-9]*' "$1" | head -1 | cut -d' ' -f2; }
convert showcase.md out.pdf >/dev/null || {
    echo "smoke.sh: FAILED: convert exited with an error" >&2
    exit 1
}
converted=$(pages "$work/convert/out.pdf")
printed=$(pages "$work/print/out.pdf")
if [ -z "$converted" ] || [ "$converted" -lt 2 ] || [ "$converted" != "$printed" ]; then
    echo "smoke.sh: FAILED: convert gave ${converted:-no} pages, print gave ${printed:-no}" >&2
    exit 1
fi
for bad in "showcase.md out.html" "missing.md out.pdf" "showcase.md"; do
    # shellcheck disable=SC2086
    if convert $bad >/dev/null; then
        echo "smoke.sh: FAILED: convert $bad succeeded" >&2
        exit 1
    fi
done
echo "    SMOKE OK ($converted pages, as printed; bad arguments rejected)"

run disk showcase.md
run autosave showcase.md
grep -q "^Quitting. Still paused. " "$work/autosave/showcase.md" || {
    echo "smoke.sh: FAILED: quitting didn't autosave" >&2
    exit 1
}
run rename showcase.md
echo "All smoke tests passed, in $(($(date +%s) - suite_started)) s."
