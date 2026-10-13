# Kami

A simple Markdown viewer and editor where reading and writing happen in the same pane.

Most Markdown tools split the screen: raw source on one side, rendered preview on the other. Kami shows one document. It looks rendered, and you edit it in place. Click into the text and you're editing it. No mode switch and no second pane.

> **Status:** early planning. No code yet. See [REQUIREMENTS.md](REQUIREMENTS.md) for the working spec and open questions.

## Goals

- **One pane.** The document you read is the document you edit.
- **Plain files.** Kami opens and saves ordinary `.md` files. No proprietary format, no lock-in, no hidden metadata.
- **Simple.** A small, focused tool, not a notes app or a knowledge base.
- **Faithful.** Saving a file you didn't change leaves it byte-for-byte identical. Edits touch only what you edited.

## Non-goals (for now)

- Folder or project sidebar (Kami opens individual files)
- Sync, accounts, or cloud storage
- Plugins or extension APIs
- Collaboration or real-time multi-user editing
- WYSIWYG features that have no Markdown equivalent

## Getting started

Kami currently builds on macOS only. Windows and Linux support is planned for a later milestone.

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
theme = "dark"       # "light", "dark" or "system" (the default)
layout = "windows"   # "tabs" (the default) or "windows", from the next start
text_width = "wide"  # "standard" (the default), "wide" or "full"
font_size = 18       # 10 to 32; the default is 16
```

The View menu changes `theme` and `text_width` too.

### Dependencies

The About tab lists the open-source libraries Kami uses. After adding or updating a dependency, run `scripts/acknowledgements.sh` to regenerate the list.

### Checks

`scripts/check.sh` runs every check a release must pass: formatting, clippy, the tests, and `cargo-deny`, which checks dependency licenses against the allow-list in `deny.toml` (REQUIREMENTS T6–T7), security advisories, and where crates come from. It needs `cargo install --locked cargo-deny`.

### Installing Kami.app

```sh
scripts/bundle-macos.sh             # builds target/release/Kami.app
scripts/bundle-macos.sh --install   # ...and installs it in ~/Applications
scripts/bundle-macos.sh --release   # a release: runs every check on a clean checkout of
                                    # the current commit, then builds from it
```

The app runs on Apple Silicon Macs, and opens Markdown files from Finder, Open With and the Dock. It's signed for the Mac it was built on only. Distributing it to others needs a Developer ID signature and notarization, which is planned for later.

Kami → Settings… can install a `kami` command for the terminal (`kami notes.md` opens files in Kami). It's a link at `/usr/local/bin/kami` to a script inside Kami.app, `macos/kami`.

The icon is built from `macos/icon-light.svg` and `macos/icon-dark.svg`. After changing either, run `scripts/make-icon.sh`. Finder shows the light icon. While Kami runs, its Dock icon follows the system's light or dark mode.

## License

_TBD_
