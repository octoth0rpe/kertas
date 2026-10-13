#!/bin/sh
# Writes src/builtin/acknowledgements.md, the list of open-source libraries
# in Kami's About tab, from Cargo.lock. Run it after changing dependencies.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out=src/builtin/acknowledgements.md

{
    echo "| Library | Version | License |"
    echo "|---|---|---|"
    cargo tree --edges normal --target aarch64-apple-darwin --prefix none \
        --format '{p}|{l}' 2>/dev/null |
        sed 's/ (\*)$//' |
        sort -u |
        awk -F'|' '
            {
                split($1, package, " ")
                name = package[1]
                version = substr(package[2], 2)
                license = $2
                if (name == "kami") next
                if (license == "") license = "See the library"
                print "| " name " | " version " | " license " |"
            }'
} > "$out"
echo "Wrote $out ($(($(wc -l < "$out") - 2)) libraries)"
