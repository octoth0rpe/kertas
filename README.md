# kami

A simple Markdown viewer and editor where reading and writing happen in the same pane.

Most Markdown tools split the screen: raw source on one side, rendered preview on the other. kami shows one document. It looks rendered, and you edit it in place. Click into the text and you're editing it. No mode switch and no second pane.

> **Status:** early planning. No code yet. See [REQUIREMENTS.md](REQUIREMENTS.md) for the working spec and open questions.

## Goals

- **One pane.** The document you read is the document you edit.
- **Plain files.** kami opens and saves ordinary `.md` files. No proprietary format, no lock-in, no hidden metadata.
- **Simple.** A small, focused tool, not a notes app or a knowledge base.
- **Faithful.** Saving a file you didn't change leaves it byte-for-byte identical. Edits touch only what you edited.

## Non-goals (for now)

- Folder or project sidebar (kami opens individual files)
- Sync, accounts, or cloud storage
- Plugins or extension APIs
- Collaboration or real-time multi-user editing
- WYSIWYG features that have no Markdown equivalent

## Getting started

kami currently builds on macOS only. Windows and Linux support is planned for a later milestone.

Requirements:
- Rust 1.98 or newer. `rust-toolchain.toml` pins the exact version for `rustup` users.
- Xcode Command Line Tools (`xcode-select --install`). Full Xcode is not required, because GPUI compiles its Metal shaders at runtime.

```sh
cargo run
```

The first build downloads GPUI from the Zed repository and takes a few minutes.

### Settings

Settings live in `~/Library/Application Support/kami/config.toml`. The file is optional, and edits to it apply straight away:

```toml
theme = "dark"     # "light", "dark" or "system" (the default)
layout = "windows" # "tabs" (the default) or "windows"
```

### Installing Kami.app

```sh
scripts/bundle-macos.sh             # builds target/release/Kami.app
scripts/bundle-macos.sh --install   # ...and installs it in ~/Applications
```

The app runs on Apple Silicon Macs, and opens Markdown files from Finder, Open With and the Dock. It's signed for the Mac it was built on only. Distributing it to others needs a Developer ID signature and notarization, which is planned for later.

The icon is built from `macos/icon.svg`. After changing the SVG, run `scripts/make-icon.sh` to rebuild `macos/AppIcon.icns`.

## License

_TBD_
