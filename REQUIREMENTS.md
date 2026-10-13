# Kami — Requirements

Working spec. Items are tagged **MUST** (v1), **SHOULD** (v1 if feasible), or **LATER** (post-v1). Open questions are collected at the end.

## 1. Core concept: single-pane editing

| ID | Requirement | Priority |
|----|-------------|----------|
| C1 | The document is shown in one pane that is both the rendered view and the editor. There is no separate preview pane. | MUST |
| C2 | Rendered elements (headings, emphasis, lists, links, code, etc.) appear formatted while you read. | MUST |
| C3 | The user can place the cursor anywhere in the document and edit directly, with no explicit "edit mode" toggle. | MUST |
| C4 | **Per-block source reveal.** The block containing the cursor (or touched by the selection) is shown as raw Markdown; all other blocks are rendered. When the cursor leaves a block, it re-renders. See §1.1. | MUST |
| C7 | Revealing or hiding a block must not shift the cursor's position in the text, and should minimize layout jump of surrounding content. | MUST |
| C5 | A read-only mode that prevents accidental edits. | SHOULD |
| C6 | A toggle to view the full raw source of the document. | SHOULD |

### 1.1 What counts as a block

A block is a top-level Markdown element:

- Paragraph
- Heading
- List (the whole list, including nested items)
- Blockquote (the whole quote)
- Fenced or indented code block
- Table
- Thematic break (`---`)
- HTML block
- Front matter (only at the very start of the file)
- Link reference definition (`[label]: https://…`). These never render, so revealing them as a block is the only way to see or edit them.

A selection that spans several blocks reveals all of them.

## 2. Markdown support

| ID | Requirement | Priority |
|----|-------------|----------|
| M1 | CommonMark compliance. | MUST |
| M2 | GitHub Flavored Markdown extensions: tables, task lists, strikethrough, autolinks. | MUST |
| M3 | Fenced code blocks with syntax highlighting. | SHOULD |
| M4 | Clickable task-list checkboxes that update the source. | SHOULD |
| M5 | Inline and linked images, with relative paths resolved against the file's location. | MUST |
| M6 | YAML front matter is preserved; it can be hidden or shown collapsed. | SHOULD |
| M7 | Math (KaTeX), Mermaid diagrams, footnotes. | LATER |

## 3. File handling

| ID | Requirement | Priority |
|----|-------------|----------|
| F1 | Open, edit, and save local `.md` / `.markdown` files. | MUST |
| F2 | Round-trip fidelity: opening and saving without edits produces an identical file. Edits do not reformat untouched parts of the document. | MUST |
| F3 | Create a new document. | MUST |
| F4 | Unsaved-changes indicator, and a prompt before closing with unsaved changes. | MUST |
| F5 | Detect when the file changes on disk and offer to reload. | SHOULD |
| F6 | Autosave. | LATER |
| F7 | Open a file from the command line (e.g. `kami notes.md`). | SHOULD |
| F8 | Recent files list. | LATER |

## 4. Editing experience

| ID | Requirement | Priority |
|----|-------------|----------|
| E1 | Standard text editing: undo/redo, cut/copy/paste, select all. | MUST |
| E2 | Keyboard shortcuts for common formatting (bold, italic, link, code, heading levels). | SHOULD |
| E3 | Smart list continuation: Enter continues a list, Enter on an empty item ends it. | SHOULD |
| E4 | Paste of rich text (e.g. from a browser) is converted to Markdown. | LATER |
| E5 | Find and replace. | SHOULD |
| E6 | Clicking a link while reading follows it; editing the link text does not. (Mechanism TBD, e.g. Cmd/Ctrl-click.) | MUST |

## 5. Appearance

| ID | Requirement | Priority |
|----|-------------|----------|
| A1 | Clean, readable default typography with a constrained line width. | MUST |
| A2 | Light and dark themes, following the OS setting by default. | MUST |
| A3 | Adjustable font size / zoom. | SHOULD |
| A4 | Optional outline / table of contents built from headings. | LATER |

## 6. Non-functional

| ID | Requirement | Priority |
|----|-------------|----------|
| N1 | Opens a typical document (< 1 MB) in under 500 ms. | MUST |
| N2 | Typing stays responsive (no visible lag) on documents up to ~10k lines. | MUST |
| N3 | Works offline; makes no network requests except to load remote images in the document. | MUST |
| N4 | Rendered HTML is sanitized; documents cannot run scripts. | MUST |
| N5 | Basic accessibility: keyboard navigable, screen-reader friendly structure. | SHOULD |

## 7. Technology

| ID | Requirement | Priority |
|----|-------------|----------|
| T1 | Written in Rust. | MUST |
| T2 | UI built with GPUI (the GPU-accelerated framework from Zed). See [docs/planning/gpui-research.md](docs/planning/gpui-research.md). | MUST |
| T3 | No HTML, JavaScript or embedded web views. All rendering is native through GPUI. | MUST |
| T4 | Runs on macOS, Windows and Linux (Wayland and X11). | MUST |
| T5 | High performance is a primary goal: GPU-rendered, smooth scrolling, no perceptible input latency (see N1 and N2). | MUST |
| T6 | **No GPL code.** No dependency may be GPL, LGPL or AGPL, and no code may be copied or ported from GPL sources. This specifically excludes Zed's `editor`, `markdown`, `markdown_preview`, `language`, `rope` and `text` crates. Reading them for design ideas is fine. | MUST |
| T7 | Dependencies must use licenses compatible with an Apache-2.0 project. **Allowed:** Apache-2.0, MIT, BSD-2/3-Clause, ISC, Zlib, BlueOak-1.0.0, Unicode-3.0, and similar permissive licenses; MPL-2.0 (file-level copyleft, so changes to MPL-licensed files must be shared under MPL, but our own code is unaffected); and dual licenses that include an allowed one (e.g. `MIT OR Apache-2.0`). **Not allowed:** GPL, LGPL, AGPL (T6). Anything else needs review before it is added. | MUST |
| T8 | License policy is enforced in CI with `cargo-deny`, using an explicit allow-list. | SHOULD |

## 8. Configuration

### 8.1 Mechanics

| ID | Requirement | Priority |
|----|-------------|----------|
| G1 | Settings are stored in a TOML file in the platform's standard config folder (e.g. `~/.config/kami/config.toml` on Linux, the Application Support folder on macOS, `%APPDATA%` on Windows). | MUST |
| G2 | The file is optional. Every setting has a default, so Kami runs with no config file at all. | MUST |
| G3 | Changes to the config file apply live, without restarting. | MUST |
| G4 | Invalid or unknown settings don't stop Kami from starting. It falls back to defaults and reports the problem in a visible but unobtrusive way. | MUST |
| G5 | A settings window inside the app (Kami > Settings…, ⌘,). | MUST |

### 8.2 Settings

| Setting | Values | Default | Priority |
|---------|--------|---------|----------|
| Theme | `light`, `dark`, `system` | `system` (A2) | MUST |
| Document layout | `tabs`, `windows` | `tabs` | MUST |
| Font size / zoom | number | TBD | SHOULD |
| Body font | font family | TBD | SHOULD |
| Code font | font family | TBD | SHOULD |
| Max line width | number of characters, or `none` for full width | TBD (A1) | SHOULD |
| Front matter display | `hidden`, `collapsed`, `shown` | `collapsed` | LATER |
| Link click | `click`, `modifier-click` | `modifier-click` | LATER |
| Restore open files on launch | `true` / `false` | `true` | LATER |
| Autosave | `off` or an interval | `off` | LATER |

Deliberately **not** settings:
- **Indentation and line-ending style.** Fidelity (F2) means Kami keeps whatever each file already uses.
- **How much of a list or quote to reveal (Q7).** Choose a good default rather than offering an option.

## Open questions

- ~~**Q1 — How does the single pane reveal syntax?**~~ **Decided:** everything is rendered except the block containing the cursor, which shows as raw Markdown (C4, §1.1).
- ~~**Q2 — Platform:**~~ **Decided:** native desktop app for macOS, Windows and Linux (T4).
- ~~**Q3 — Stack:**~~ **Decided:** Rust + GPUI (T1–T3). Built on plain GPUI pinned to a Zed commit, not gpui-component ([spike #3](docs/planning/spike-3-text-input.md)). Markdown parser: `pulldown-cmark`.
- ~~**Q4 — Scope of files:**~~ **Decided:** individual files only, with no folder sidebar. A sidebar may come in a possible future "pro" version.
- ~~**Q5 — Tabs / multiple windows**~~ **Decided:** both are supported, chosen by a setting (§8.2). The default is tabs.
- **Q6 — Kami's own license.** Not yet decided; leaning Apache-2.0. Decide before publishing or accepting outside contributions. Also consider the possible pro version. As the copyright holder, you can build a closed pro version under any license. The license decides whether others can do the same (a permissive license allows it, GPL does not), and on what terms outside contributions can go into the pro version (a contributor agreement may be needed). Dependencies are constrained by T6–T7 regardless.
- ~~**Q7 — How big is a block for lists and blockquotes?**~~ **Decided (for now):** the whole list or blockquote is revealed. Revisit if large lists turn out to be disruptive in practice.
