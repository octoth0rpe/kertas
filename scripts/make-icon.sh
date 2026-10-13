#!/bin/sh
# Builds Kertas's icons from the SVGs in macos/, using only tools that come
# with macOS. Run it again after changing either SVG.
#
# - macos/AppIcon.icns, from icon-light.svg: the app's icon in Finder.
# - macos/dock-light.png and dock-dark.png: the Dock icon, which Kertas
#   switches to follow the system's light or dark mode while running.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

sips -s format png "$root/macos/icon-light.svg" --out "$work/light.png" >/dev/null
sips -s format png "$root/macos/icon-dark.svg" --out "$work/dark.png" >/dev/null

iconset="$work/AppIcon.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$work/light.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$work/light.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$root/macos/AppIcon.icns"

# The Dock shows icons at up to 512 points (1024 pixels) when magnified.
cp "$work/light.png" "$root/macos/dock-light.png"
cp "$work/dark.png" "$root/macos/dock-dark.png"
echo "Wrote macos/AppIcon.icns, dock-light.png and dock-dark.png"
