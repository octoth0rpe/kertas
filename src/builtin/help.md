# Kami Help

Kami shows a Markdown file as it will look, and lets you edit it right there. There's no separate preview.

## Editing in place

Click any block, such as a paragraph, heading, list or table, to edit it. That block shows its Markdown, with the formatting characters dimmed, and everything else stays rendered. Move away and it renders again.

- **Esc** finishes editing.
- **⌥↓** and **⌥↑** move to the next or previous block.
- Clicking in the margins also finishes editing.

## Writing

- **↩** starts a new line. A single line break inside a paragraph shows as a space, as in all Markdown, so press **↩** twice to start a new paragraph.
- **⇧↩** adds a line break that shows, within a paragraph.
- In a list, **↩** starts the next item, with the next number or an empty task box. On an empty item, it ends the list. Quotes work the same way.
- **⇥** and **⇧⇥** indent and outdent list items, and code in code blocks.

See **Help → Keyboard Shortcuts** for every shortcut.

## Tabs and windows

Each document opens in its own tab. **⌘1** to **⌘9** go straight to a tab. To open documents in separate windows instead, set `layout = "windows"` in the settings file.

## Appearance

**View → Appearance** chooses Light, Dark or Match System. **View → Text Width** chooses how wide the text is: Standard, Wide, or the whole window.

## Settings

Settings are kept in `~/Library/Application Support/kami/config.toml`. The View menu changes them for you, and you can also edit the file. Changes apply straight away.

```toml
theme = "dark"       # "light", "dark" or "system"
layout = "windows"   # "tabs" or "windows"
text_width = "wide"  # "standard", "wide" or "full"
```

If the file has a mistake, Kami uses the default for that setting and shows what's wrong at the bottom of the window.

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
