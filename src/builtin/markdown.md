# Markdown Syntax

Kertas supports CommonMark and GitHub's additions to it: tables, task lists, strikethrough, links from web addresses, footnotes and alerts. Each example shows the Markdown, then how it looks. The everyday syntax comes first.

## Headings

```markdown
# Heading 1
## Heading 2
### Heading 3
```

There are six levels, from `#` to `######`. A space after the `#`s is needed: `#Heading` is just text.

## Emphasis

```markdown
*italic*, **bold**, ***both***, ~~strikethrough~~ and `code`
```

*italic*, **bold**, ***both***, ~~strikethrough~~ and `code`

Underscores work too: `_italic_` and `__bold__`. Inside a word, only `*` works, so `snake_case_name` stays as it is.

To put a backtick in code, wrap it in two: ``` `` a`b `` ``` shows `` a`b ``.

## Paragraphs and line breaks

A blank line starts a new paragraph. A single line break inside a paragraph shows as a space. To break the line there, end it with `\` (**⇧↩** does this for you), or with two spaces.

```markdown
Roses are red,\
violets are blue.
```

Roses are red,\
violets are blue.

## Links

```markdown
[Markdown](https://commonmark.org)
```

[Markdown](https://commonmark.org)

Web addresses and email addresses become links by themselves, as on GitHub. Wrapping one in `<` and `>` does the same.

```markdown
https://commonmark.org
```

https://commonmark.org

A link can have a title after its address, `[Markdown](https://commonmark.org "CommonMark")`, which Kertas keeps but doesn't show.

## Images

```markdown
![A description of the picture](pictures/photo.png)
```

The path is relative to the document's folder. The image is shown at its own size, or smaller to fit, and the description is shown instead if the file can't be found. Images from the web aren't loaded.

## Lists

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

Bullets can also be `*` or `+`, and numbers can end with `)` instead of `.`. Indent a nested item to line up with the text of the item above it.

## Quotes

```markdown
> A quote.
>
> > A quote within a quote.
```

> A quote.
>
> > A quote within a quote.

## Code

Fence code with three backticks, or three tildes (`~~~`). A language name after the opening fence is kept, but code isn't coloured.

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

## Tables

```markdown
| Left | Center | Right |
|:-----|:------:|------:|
| a    | **b**  | `c`   |
```

| Left | Center | Right |
|:-----|:------:|------:|
| a    | **b**  | `c`   |

The colons in the second row align each column. The pipes don't need to line up, and the outer ones can be left out. To put a `|` in a cell, write `\|`.

## Lines

```markdown
---
```

---

`***` and `___` work too. A blank line is needed before a `---` line, or it underlines a heading (see below).

## Underlined headings

```markdown
Heading 1
=========

Heading 2
---------
```

Heading 1
=========

Heading 2
---------

## Numbered lists that start elsewhere

A list starts at its first number. The numbers after it don't matter: Kertas counts on from the first.

```markdown
3. Third
1. Fourth
1. Fifth
```

3. Third
1. Fourth
1. Fifth

## Indented code

Lines indented by four spaces, after a blank line, are code too.

```markdown
    let indented = true;
```

    let indented = true;

## Escapes

A backslash before a punctuation character shows the character itself, rather than the formatting it would start.

```markdown
\*not italic\*, \# not a heading, 1\. not a list
```

\*not italic\*, \# not a heading, 1\. not a list

## Reference links

A link can name its address with a label, and give the address anywhere else in the file. The definition itself isn't rendered: it shows as Markdown.

```markdown
Read the [CommonMark spec][spec], or just [spec].

[spec]: https://spec.commonmark.org
```

Read the [CommonMark spec][spec], or just [spec].

[spec]: https://spec.commonmark.org

Labels ignore case. Images work the same way: `![A photo][photo]`.

## Links within a document

A link to `#` and a heading's name jumps to that heading. The name is the heading in lowercase, with punctuation removed and spaces turned into `-`, as on GitHub: `[Tables](#tables)` jumps to the [Tables](#tables) heading above. `other.md#setup` opens other.md at its Setup heading.

To link to a spot that isn't a heading, mark it with `<a id="spot"></a>` and link to `#spot`. The mark doesn't show.

## Alerts

A quote that starts with `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` or `[!CAUTION]` on a line of its own is an alert, as on GitHub.

```markdown
> [!TIP]
> Alerts stand out from the text around them.
```

> [!TIP]
> Alerts stand out from the text around them.

## Footnotes

```markdown
Kertas shows footnotes[^1] as GitHub does.

[^1]: The note can go anywhere in the file.
```

Kertas shows footnotes[^1] as GitHub does.

[^1]: The note can go anywhere in the file.

Each note is drawn at the end of the document, numbered in the order the references appear. Click a number to jump to its note, and the ↩ after a note to jump back. Click a note to edit it where it is in the file.

## Front matter

Settings for other tools, such as a blog's title and date, can go at the very start of the file, between two `---` lines (YAML) or two `+++` lines (TOML). Kertas keeps the front matter and shows it as it's written.

```markdown
---
title: My post
date: 2026-10-16
---
```

## HTML

HTML is shown as it's written, never run or drawn as a web page. The exception is a link target like `<a id="spot"></a>`, which doesn't show (see [Links within a document](#links-within-a-document)).

```markdown
Press <kbd>⌘S</kbd> to save.
```

Press <kbd>⌘S</kbd> to save.

## Not supported

- **Coloured code:** code blocks are drawn in the code font, without syntax highlighting.
- **Math**, such as `$x^2$`, shows as it's written.
- **HTML** isn't rendered (see above).
