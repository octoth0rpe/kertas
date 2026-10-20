#!/bin/sh
# Runs the typing benchmark (the `bench` mode in src/smoke.rs) against a
# debug build, on a scratch copy of tests/fixtures/10k.md, and fails if
# inserting or drawing is slower than the baselines in the README.
#
#   scripts/bench.sh
#
# It opens a Kertas window for about 20 seconds; keep it in front.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo build --quiet
kertas="${CARGO_TARGET_DIR:-$root/target}/debug/kertas"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/config"
cp "$root/tests/fixtures/10k.md" "$work/10k.md"
echo "==> bench (10k.md)"
if ! output=$(env KERTAS_SMOKE=bench KERTAS_CONFIG_DIR="$work/config" \
    perl -e 'alarm shift; exec @ARGV' 300 "$kertas" "$work/10k.md" 2>&1); then
    printf '%s\n' "$output" | tail -20
    echo "bench.sh: FAILED: Kertas exited with an error" >&2
    exit 1
fi
printf '%s\n' "$output" | grep -E "^SMOKE|: (ok|FAILED)|p50" | sed 's/^/    /'
printf '%s\n' "$output" | grep -q "^SMOKE OK" || {
    echo "bench.sh: FAILED" >&2
    exit 1
}
