#!/bin/sh
# Runs every smoke test mode (src/smoke.rs) against a debug build, each on
# a scratch copy of a fixture with its own empty settings folder, and stops
# at the first failure.
#
#   scripts/smoke.sh
#
# The smoke tests open Kertas windows and take keyboard focus for about two
# minutes, so leave the keyboard alone while they run.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo build --quiet
kertas="${CARGO_TARGET_DIR:-$root/target}/debug/kertas"
fixtures="$root/spikes/fixtures"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Runs one mode on a copy of a fixture, with a time limit. Extra
# environment variables can follow the fixture name. Prints the output and
# succeeds only if it says "SMOKE OK" (or, with --exits, if Kertas quit by
# itself).
run() {
    mode=$1
    fixture=$2
    shift 2
    expect="SMOKE OK"
    if [ "${1:-}" = "--exits" ]; then
        expect=""
        shift
    fi
    dir="$work/$mode"
    mkdir -p "$dir/config"
    cp "$fixtures/$fixture" "$dir/$fixture"
    cp -R "$fixtures/images" "$dir/images"
    echo "==> $mode ($fixture)"
    # macOS has no timeout(1); perl's alarm kills Kertas if it hangs.
    if ! output=$(env KERTAS_SMOKE="$mode" KERTAS_CONFIG_DIR="$dir/config" "$@" \
        perl -e 'alarm shift; exec @ARGV' 300 "$kertas" "$dir/$fixture" 2>&1); then
        echo "$output" | tail -20
        echo "smoke.sh: FAILED: $mode exited with an error" >&2
        exit 1
    fi
    echo "$output" | grep -E "^SMOKE|: (ok|FAILED)|p50" | sed 's/^/    /'
    if [ -n "$expect" ] && ! echo "$output" | grep -q "^$expect"; then
        echo "smoke.sh: FAILED: $mode" >&2
        exit 1
    fi
}

run 1 showcase.md
run 1 10k.md
run save showcase.md
grep -q "Saved by smoke test." "$work/save/showcase.md" || {
    echo "smoke.sh: FAILED: save didn't write the file" >&2
    exit 1
}

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
run settings showcase.md
run settings-window showcase.md
run help showcase.md
run reveal showcase.md
run clicks clicks.md
run anchors anchors.md
run caret anchors.md
run reading anchors.md
run scrollbar 10k.md
run scrollbar-drag showcase.md
run images showcase.md
run a11y a11y.md
run print showcase.md KERTAS_PRINT_PDF="$work/print/out.pdf"
run disk showcase.md
run autosave showcase.md
grep -q "^Quitting. Still paused. " "$work/autosave/showcase.md" || {
    echo "smoke.sh: FAILED: quitting didn't autosave" >&2
    exit 1
}
run quit showcase.md --exits
run bench 10k.md
echo "All smoke tests passed."
