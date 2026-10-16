#!/bin/sh
# Builds Kertas.app in target/release/, signed for this Mac only.
#
#   scripts/bundle-macos.sh             # build target/release/Kertas.app
#   scripts/bundle-macos.sh --install   # ...and replace ~/Applications/Kertas.app,
#                                       # quitting it first if it's running
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
# The kt command line tool, which Settings links into /usr/local/bin.
cp macos/kt "$app/Contents/Resources/kt"
chmod +x "$app/Contents/Resources/kt"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Ad-hoc signature: enough to run on this Mac. Distributing to others needs
# a Developer ID signature and notarization.
codesign --force --sign - "$app"
echo "Built $app ($version)"

if $install; then
    installed="$HOME/Applications/Kertas.app"
    # Quit the installed Kertas first, as ⌘Q would, so unsaved changes are
    # saved or asked about. Only that copy: a build running from target/
    # is left alone.
    running() { pgrep -f "$installed/Contents/MacOS/Kertas" >/dev/null; }
    if running; then
        echo "Quitting $installed"
        osascript -e "tell application \"$installed\" to quit" >/dev/null 2>&1 || true
        waited=0
        while running; do
            if [ "$waited" -ge 30 ]; then
                echo "bundle-macos.sh: Kertas is still running (asking about unsaved changes?); quit it and try again" >&2
                exit 1
            fi
            sleep 1
            waited=$((waited + 1))
        done
    fi
    mkdir -p "$HOME/Applications"
    rm -rf "$installed"
    cp -R "$app" "$installed"
    # Tell Launch Services about the app, so Finder offers it for Markdown.
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$installed"
    echo "Installed ~/Applications/Kertas.app"
fi
