#!/bin/sh
# Builds Kami.app in target/release/, signed for this Mac only.
#
#   scripts/bundle-macos.sh             # build target/release/Kami.app
#   scripts/bundle-macos.sh --install   # ...and copy it to ~/Applications
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo build --release

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
app="target/release/Kami.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/@VERSION@/$version/g" macos/Info.plist > "$app/Contents/Info.plist"
cp target/release/kami "$app/Contents/MacOS/kami"
cp macos/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Ad-hoc signature: enough to run on this Mac. Distributing to others needs
# a Developer ID signature and notarization.
codesign --force --sign - "$app"
echo "Built $app ($version)"

if [ "${1:-}" = "--install" ]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Kami.app"
    cp -R "$app" "$HOME/Applications/Kami.app"
    # Tell Launch Services about the app, so Finder offers it for Markdown.
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$HOME/Applications/Kami.app"
    echo "Installed ~/Applications/Kami.app"
fi
