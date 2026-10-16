# Kertas Help

Kertas shows a Markdown file as it will look, and lets you edit it right there. There's no separate preview.

## Editing in place

Click any block, such as a paragraph, heading, list or table, to edit it. That block shows its Markdown, with the formatting characters dimmed, and everything else stays rendered. Move away and it renders again.

- **Esc** finishes editing.
- **⌥↓** and **⌥↑** move to the next or previous block.
- Clicking in the margins also finishes editing.

Clicking a link follows it. Links to Markdown files open in Kertas, and other links open in their own apps: web pages in your browser, for example. To edit a link, click elsewhere in its block, or move to it with **⌥↓** and **⌥↑**.

Clicking a task's checkbox (☐) ticks or unticks it. Click the task's text to edit it.

## Writing

- **↩** starts a new line. A single line break inside a paragraph shows as a space, as in all Markdown, so press **↩** twice to start a new paragraph.
- **⇧↩** adds a line break that shows, within a paragraph.
- In a list, **↩** starts the next item, with the next number or an empty task box. On an empty item, it ends the list. Quotes work the same way.
- **⇥** and **⇧⇥** indent and outdent list items, and code in code blocks.

See **Help → Keyboard Shortcuts** for every shortcut.

## Saving

**⌘S** saves, and **⇧⌘S** (Save As) saves under another name or in another folder. To save automatically, choose **After a pause** for **Autosave** in **Kertas → Settings…**. Documents then save a few seconds after you stop typing, when you switch to another tab, window or app, and when you close them or quit. A new document is only saved automatically once you've saved it the first time.

## When another app changes a file

If another app changes a document's file, such as another editor or `git checkout`, Kertas shows the new version straight away. **⌘Z** brings back what you had. If you have unsaved changes, Kertas asks first: **Reload** replaces your changes with the new file, and **Keep Mine** keeps them, so saving writes over it. If the file is deleted or moved, Kertas keeps the document open, and saving puts the file back.

## Tabs and windows

Each document opens in its own tab. **⌘1** to **⌘9** go straight to a tab. To open documents in separate windows instead, choose **Windows** for "Open documents in" in **Kertas → Settings…**. That takes effect the next time Kertas starts.

## Printing

**File → Print…** (⌘P) prints the document as it looks rendered, in light colors. To save a PDF, use the PDF menu in the print panel.

## From the terminal

**Kertas → Install Command Line Tool…** installs the `kt` command. Then `kt notes.md` opens a file in Kertas. A file that doesn't exist yet opens as a new document, and saving creates it.

## Appearance

**View → Appearance** chooses Light, Dark or Match System. **View → Text Width** chooses how wide the text is: Standard, Wide, or the whole window.

**⌘+** and **⌘-** make the text bigger or smaller in every document, and **⌘0** returns to your usual size. Zooming lasts until you quit. To change the size documents start at, use **Default text size** in **Kertas → Settings…**.

## Settings

Settings are kept in `~/Library/Application Support/kertas/config.toml`. **Kertas → Settings…** (⌘,) and the View menu change them for you, and you can also edit the file. Changes apply straight away, except `layout`, which applies the next time Kertas starts.

```toml
theme = "dark"        # "light", "dark" or "system"
layout = "windows"    # "tabs" or "windows", from the next start
text_width = "wide"   # "standard", "wide" or "full"
font_size = 18        # 10 to 32
body_font = "Charter" # any installed font; the system font if not set
code_font = "SF Mono" # Menlo if not set
tab_width = 2         # how wide tab characters in code look: 2, 4 or 8 spaces
list_indent = "wide"  # how far nested list items are drawn in: "compact", "standard" or "wide"
editing_hint = false  # hide keyboard hints on the block being edited
autosave = 5          # save 5 seconds after typing stops; "off" if not set
```

If the file has a mistake, Kertas uses the default for that setting and shows what's wrong at the bottom of the window.

**Tab character width** only changes how tab characters in code look. It doesn't change what the Tab key inserts or how lists are indented. **List indent** only changes how far nested list items are drawn in; the Markdown keeps whatever indentation it has.

To make Kertas open Markdown files when you double-click them in Finder, choose **Kertas → Make Kertas the Default Markdown App**.

## Markdown guide

Each example shows the Markdown, then how it looks.

### Headings

```markdown
# Heading 1
## Heading 2
### Heading 3
```

### Emphasis

```markdown
*italic*, **bold**, ~~strikethrough~~ and `code`
```

*italic*, **bold**, ~~strikethrough~~ and `code`

### Links

```markdown
[Markdown](https://commonmark.org)
```

[Markdown](https://commonmark.org)

A link to `#` and a heading's name jumps to that heading. The name is the heading in lowercase, with punctuation removed and spaces turned into `-`, as on GitHub: `[Tables](#tables)` jumps to the Tables heading below. `other.md#setup` opens other.md at its Setup heading.

Web addresses and email addresses become links by themselves, as on GitHub:

```markdown
https://commonmark.org
```

https://commonmark.org

### Lists

```markdown
- Bullets
- With items
  - And nested items

1. Numbered
2. Items

- [x] Done
- [ ] To do
```

- Bullets
- With items
  - And nested items

1. Numbered
2. Items

- [x] Done
- [ ] To do

### Quotes

```markdown
> A quote.
```

> A quote.

### Alerts

A quote that starts with `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` or `[!CAUTION]` on a line of its own is an alert, as on GitHub.

```markdown
> [!TIP]
> Alerts stand out from the text around them.
```

> [!TIP]
> Alerts stand out from the text around them.

### Code

````markdown
```rust
fn main() {
    println!("Hello");
}
```
````

```rust
fn main() {
    println!("Hello");
}
```

### Tables

```markdown
| Left | Center | Right |
|:-----|:------:|------:|
| a    |   b    |     c |
```

| Left | Center | Right |
|:-----|:------:|------:|
| a    |   b    |     c |

### Lines

```markdown
---
```

---
