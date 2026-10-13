#!/bin/sh
# Builds macos/AppIcon.icns from macos/icon.svg, using only tools that come
# with macOS. Run it again after changing the SVG.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
svg="$root/macos/icon.svg"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

iconset="$work/AppIcon.iconset"
mkdir "$iconset"
sips -s format png "$svg" --out "$work/icon-1024.png" >/dev/null
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$work/icon-1024.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$work/icon-1024.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$root/macos/AppIcon.icns"
echo "Wrote macos/AppIcon.icns"
