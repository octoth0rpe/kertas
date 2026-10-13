#!/bin/sh
# Builds Kertas.app in target/release/, signed for this Mac only.
#
#   scripts/bundle-macos.sh             # build target/release/Kertas.app
#   scripts/bundle-macos.sh --install   # ...and copy it to ~/Applications
#   scripts/bundle-macos.sh --release   # build from a clean checkout of the
#                                       # current commit, after every check
#                                       # (scripts/check.sh) and smoke test
#                                       # (scripts/smoke.sh) passes
#
# The options can be combined: --release --install.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

release=false
install=false
for arg in "$@"; do
    case $arg in
        --release) release=true ;;
        --install) install=true ;;
        *) echo "bundle-macos.sh: unknown option $arg" >&2; exit 2 ;;
    esac
done

# Build artifacts always go in this checkout's target/, so a release build
# reuses compiled dependencies.
export CARGO_TARGET_DIR="$root/target"

if $release; then
    # A release is built from the commit, never from uncommitted changes.
    if [ -n "$(git status --porcelain)" ]; then
        echo "Note: uncommitted changes are not included in a release build."
    fi
    worktree="$root/target/release-worktree"
    git worktree remove --force "$worktree" 2>/dev/null || rm -rf "$worktree"
    git worktree add --detach --quiet "$worktree" HEAD
    trap 'git -C "$root" worktree remove --force "$worktree"' EXIT
    echo "Release build of $(git rev-parse --short HEAD)"
    "$worktree/scripts/check.sh"
    # The smoke tests open windows and take keyboard focus for about two
    # minutes.
    "$worktree/scripts/smoke.sh"
    cd "$worktree"
fi

cargo build --release

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
app="$root/target/release/Kertas.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/@VERSION@/$version/g" macos/Info.plist > "$app/Contents/Info.plist"
# Named Kertas, as Activity Monitor and Force Quit show it.
cp "$CARGO_TARGET_DIR/release/kertas" "$app/Contents/MacOS/Kertas"
cp macos/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
# The kertas command line tool, which Settings links into /usr/local/bin.
cp macos/kertas "$app/Contents/Resources/kertas"
chmod +x "$app/Contents/Resources/kertas"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Ad-hoc signature: enough to run on this Mac. Distributing to others needs
# a Developer ID signature and notarization.
codesign --force --sign - "$app"
echo "Built $app ($version)"

if $install; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Kertas.app"
    cp -R "$app" "$HOME/Applications/Kertas.app"
    # Tell Launch Services about the app, so Finder offers it for Markdown.
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$HOME/Applications/Kertas.app"
    echo "Installed ~/Applications/Kertas.app"
fi
