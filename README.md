# Kertas

A simple Markdown viewer and editor where reading and writing happen in the same pane.

Most Markdown tools split the screen: raw source on one side, rendered preview on the other. Kertas shows one document. It looks rendered, and you edit it in place. Click into the text and you're editing it. No mode switch and no second pane.

> **Status:** version 0.2, an early release for macOS on Apple Silicon. Kertas opens, edits and saves Markdown files in one pane. See [Installing Kertas.app](#installing-kertasapp) to build it, and [REQUIREMENTS.md](REQUIREMENTS.md) for the working spec and what's planned.

## Goals

- **One pane.** The document you read is the document you edit.
- **Plain files.** Kertas opens and saves ordinary `.md` files. No proprietary format, no lock-in, no hidden metadata.
- **Simple.** A small, focused tool, not a notes app or a knowledge base.
- **Faithful.** Saving a file you didn't change leaves it byte-for-byte identical. Edits touch only what you edited.

## Non-goals (for now)

- Folder or project sidebar (Kertas opens individual files)
- Sync, accounts, or cloud storage
- Plugins or extension APIs
- Collaboration or real-time multi-user editing
- WYSIWYG features that have no Markdown equivalent

## Getting started

Kertas currently builds on macOS only. Windows and Linux support is planned for a later milestone.

Requirements:
- Rust 1.98 or newer. `rust-toolchain.toml` pins the exact version for `rustup` users.
- Xcode Command Line Tools (`xcode-select --install`). Full Xcode is not required, because GPUI compiles its Metal shaders at runtime.

```sh
cargo run
```

The first build downloads GPUI from the Zed repository and takes a few minutes.

### Settings

Settings live in `~/Library/Application Support/kertas/config.toml`. The file is optional, and edits to it apply straight away:

```toml
theme = "dark"       # "light", "dark" or "system" (the default)
layout = "windows"   # "tabs" (the default) or "windows", from the next start
text_width = "wide"  # "standard" (the default), "wide" or "full"
font_size = 18       # 10 to 32; the default is 16
```

The View menu changes `theme` and `text_width` too.

### Dependencies

The About tab lists the open-source libraries Kertas uses, and Help → Licenses shows the license notices their licenses ask to be included. `scripts/acknowledgements.sh` generates them with [cargo-about](https://github.com/EmbarkStudios/cargo-about) (`cargo install --locked cargo-about --features cli`), configured in `scripts/about.toml`. It needs the network, since a few libraries don't include their license files and cargo-about fetches them from the libraries' repositories.

Run it after adding or updating a dependency, or let git do it: after `git config core.hooksPath scripts/hooks`, a commit that changes `Cargo.lock` regenerates the acknowledgements, and stops so you can add them. `scripts/check.sh` checks they list exactly the libraries in `Cargo.lock`.

### Checks

`scripts/check.sh` runs every check a release must pass: formatting, clippy, the tests, and `cargo-deny`, which checks dependency licenses against the allow-list in `deny.toml` (REQUIREMENTS T6–T7), security advisories, and where crates come from, and that the About tab acknowledges every library. It needs `cargo install --locked cargo-deny` and cargo-about (see above).

`scripts/smoke.sh` runs the smoke tests (`src/smoke.rs`): Kertas drives itself through editing, tabs, settings, help, revealing blocks, printing and quitting, on scratch copies of the fixtures. It takes about two and a half minutes, opens windows and takes keyboard focus, so leave the keyboard alone while it runs.

Speed is checked too, so Kertas doesn't get slower without anyone noticing. Baselines on an M2 MacBook Air (2026-10-13), debug build, with `spikes/fixtures/10k.md`:

| Check | Typical | Fails above |
|---|---|---|
| Parse and render the whole document (`large_document_renders_quickly`) | 34 ms | 150 ms |
| Update the model after one keystroke (`typing_in_large_document_is_fast`) | 13 ms | 100 ms |
| Smoke `bench`: handle one insert, 95th percentile | 14 ms | 60 ms |
| Smoke `bench`: insert until its frame is drawn, 95th percentile | 30 ms | 120 ms |

Release builds are roughly ten times faster.

### Installing Kertas.app

To build and install a release on an Apple Silicon Mac that has only [Homebrew](https://brew.sh), paste this into Terminal, with the URL of Kertas's git repository in place of `<repo-url>`:

```sh
brew install rustup
export PATH="$(brew --prefix rustup)/bin:$PATH"
git clone --branch v0.2.0 <repo-url> kertas
cd kertas
rustup toolchain install
scripts/bundle-macos.sh --install
open ~/Applications/Kertas.app
```

`rustup toolchain install` installs the Rust version that `rust-toolchain.toml` pins. The `export PATH` line lasts only for that Terminal window, since Homebrew doesn't put rustup on the `PATH`. The first build downloads every dependency and takes several minutes. A note from `git clone` about a "detached HEAD" is normal: it means you're on the release tag rather than a branch. To update later, open a new Terminal window, run the `export PATH` line, then in `kertas` run `git fetch --tags`, `git checkout <new tag>` and `scripts/bundle-macos.sh --install`.

For development:

```sh
scripts/bundle-macos.sh             # builds target/release/Kertas.app
scripts/bundle-macos.sh --install   # ...and installs it in ~/Applications
scripts/bundle-macos.sh --release   # a release: runs every check and smoke test on a clean
                                    # checkout of the current commit, then builds from it
```

The app runs on Apple Silicon Macs, and opens Markdown files from Finder, Open With and the Dock. It's signed for the Mac it was built on only. Distributing it to others needs a Developer ID signature and notarization, which is planned for later.

Kertas → Settings… can install a `kertas` command for the terminal (`kertas notes.md` opens files in Kertas). It's a link at `/usr/local/bin/kertas` to a script inside Kertas.app, `macos/kertas`.

The icon is built from `macos/icon-light.svg` and `macos/icon-dark.svg`. After changing either, run `scripts/make-icon.sh`. Finder shows the light icon. While Kertas runs, its Dock icon follows the system's light or dark mode.

## License

_TBD_
