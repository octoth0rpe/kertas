#!/bin/sh
# Every check a release must pass (REQUIREMENTS T6–T8): formatting, clippy
# (debug and release), tests, and cargo-deny (licenses, security
# advisories, crate sources). Stops at the first failure.
#
#   scripts/check.sh
#
# Needs cargo-deny: `cargo install --locked cargo-deny`.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if ! command -v cargo-deny >/dev/null 2>&1; then
    echo "check.sh: cargo-deny is missing; install it with: cargo install --locked cargo-deny" >&2
    exit 1
fi

step() {
    echo "==> $*"
    if ! "$@"; then
        echo "check.sh: FAILED: $*" >&2
        exit 1
    fi
}

step cargo fmt --check
step cargo clippy --all-targets -- -D warnings
step cargo clippy --release -- -D warnings
step cargo test
step cargo deny check
echo "All checks passed."
