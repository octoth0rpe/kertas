# Kertas

A simple Markdown viewer and editor where reading and writing happen in the same pane.

Most Markdown tools split the screen: raw source on one side, rendered preview on the other. Kertas shows one document. It looks rendered, and you edit it in place. Click into the text and you're editing it. No mode switch and no second pane.

> **Status:** version 0.4, an early release for macOS on Apple Silicon. Kertas opens, edits and saves Markdown files in one pane. See [Installing Kertas.app](#installing-kertasapp) to build it, and [REQUIREMENTS.md](REQUIREMENTS.md) for the working spec and what's planned.

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

The repository is a Cargo workspace. The app is the package at the root. `crates/kertas-core` holds the document model, Markdown parsing and settings, without any UI. `crates/kertas-editor` is the GPUI view that shows and edits a document, which another app can put in its own windows; `cargo run -p kertas-editor --example embed` shows one on its own. It also holds what such an app can share with Kertas: the settings file and themes (in a folder the app chooses), zoom, the View menu, the settings window, the bar reporting problems, and the Markdown Syntax, Themes and editor shortcuts help texts.

### Settings

Settings live in `~/Library/Application Support/kertas/config.toml`. The file is optional, and edits to it apply straight away:

```toml
appearance = "dark"  # "light", "dark" or "system" (the default)
theme = "Kertas"     # the color theme: Kertas (the default), Solarized, Gruvbox, Catppuccin or your own
layout = "windows"   # "tabs" (the default) or "windows", from the next start
text_width = "wide"  # "standard" (the default), "wide" or "full"
font_size = 18       # 10 to 32; the default is 16
fade_other_blocks = true  # fade every block but the one being edited; off by default
typewriter_scrolling = true  # keep the caret's line in the middle of the window; off by default
```

The View menu changes `appearance`, `theme`, `text_width` and `scrollbar` too. `fade_other_blocks` and `typewriter_scrolling` are changed only there, or in the file, to keep the Settings window short.

Your own color themes go in `themes/` next to `config.toml`, one `.toml` file per theme with its light and dark colors, and are listed with the built-in ones. A theme with a built-in theme's name replaces it. **Help → Themes** in the app lists every color and has a complete example.

### Dependencies

The About tab lists the open-source libraries Kertas uses, and Help → Licenses shows the license notices their licenses ask to be included. `scripts/acknowledgements.sh` generates them with [cargo-about](https://github.com/EmbarkStudios/cargo-about) (`cargo install --locked cargo-about --features cli`), configured in `scripts/about.toml`. It needs the network, since a few libraries don't include their license files and cargo-about fetches them from the libraries' repositories.

Run it after adding or updating a dependency, or let git do it: after `git config core.hooksPath scripts/hooks`, a commit that changes `Cargo.lock` regenerates the acknowledgements, and stops so you can add them. `scripts/check.sh` checks they list exactly the libraries in `Cargo.lock`.

### Checks

`scripts/check.sh` runs every check a release must pass: formatting, clippy, the tests, and `cargo-deny`, which checks dependency licenses against the allow-list in `deny.toml` (REQUIREMENTS T6–T7), security advisories, and where crates come from, and that the About tab acknowledges every library. It needs `cargo install --locked cargo-deny` and cargo-about (see above).

`scripts/smoke.sh` runs the smoke tests (`src/smoke.rs`): Kertas drives itself through editing, tabs, settings, help, revealing blocks, printing and quitting, on scratch copies of the fixtures. It takes about two and a half minutes, opens windows and takes keyboard focus, so leave the keyboard alone while it runs.

Speed is checked too, so Kertas doesn't get slower without anyone noticing. Baselines on an M2 MacBook Air (2026-10-13), debug build, with `tests/fixtures/10k.md`:

| Check | Typical | Fails above |
|---|---|---|
| Parse and render the whole document (`large_document_renders_quickly`) | 34 ms | 150 ms |
| Update the model after one keystroke (`typing_in_large_document_is_fast`) | 13 ms | 100 ms |
| Smoke `bench`: handle one insert, 95th percentile | 14 ms | 60 ms |
| Smoke `bench`: insert until its frame is drawn, 95th percentile | 30 ms | 120 ms |

Release builds are roughly ten times faster.

### Installing Kertas.app

To build and install a release on an Apple Silicon Mac that has only [Homebrew](https://brew.sh), paste this into Terminal:

```sh
brew install rustup
export PATH="$(brew --prefix rustup)/bin:$PATH"
git clone https://github.com/octoth0rpe/kertas.git
cd kertas
rustup toolchain install
scripts/bundle-macos.sh --install
open ~/Applications/Kertas.app
```

`rustup toolchain install` installs the Rust version that `rust-toolchain.toml` pins. The `export PATH` line lasts only for that Terminal window, since Homebrew doesn't put rustup on the `PATH`. The first build downloads every dependency and takes several minutes. The `main` branch is always the latest release, and each release also has a tag, such as `v0.4.0`. To update later, open a new Terminal window, run the `export PATH` line, then in `kertas` run `git pull` and `scripts/bundle-macos.sh --install`.

For development:

```sh
scripts/bundle-macos.sh             # builds target/release/Kertas.app
scripts/bundle-macos.sh --install   # ...and installs it in ~/Applications, quitting
                                    # the installed copy first (also: make install)
scripts/bundle-macos.sh --release   # a release: runs every check and smoke test on a clean
                                    # checkout of the current commit, then builds from it
```

The app runs on Apple Silicon Macs, and opens Markdown files from Finder, Open With and the Dock. It's signed for the Mac it was built on only. Distributing it to others needs a Developer ID signature and notarization, which is planned for later.

Kertas → Settings… can install a `kt` command for the terminal (`kt notes.md` opens files in Kertas). It's a link at `/usr/local/bin/kt` to a script inside Kertas.app, `macos/kt`.

The icon is built from `macos/icon-light.svg` and `macos/icon-dark.svg`. After changing either, run `scripts/make-icon.sh`. Finder shows the light icon. While Kertas runs, its Dock icon follows the system's light or dark mode.

## License

Kertas is under the MIT License (`LICENSE`), which covers every revision of this repository, including those from before `LICENSE` was added. The libraries and other open-source work it includes keep their own licenses, listed in the About tab and Help → Licenses (see above). The alert icons are GitHub's Octicons, also MIT (`crates/kertas-editor/assets/alerts/LICENSE`).
