# Kertas Help

Kertas shows a Markdown file as it will look, and lets you edit it right there. There's no separate preview.

## Editing in place

Click any block, such as a paragraph, heading, list or table, to edit it. That block shows its Markdown, with the formatting characters dimmed, and everything else stays rendered. Move away and it renders again.

- **Esc** finishes editing.
- **⌥↓** and **⌥↑** move to the next or previous block.
- Clicking in the margins also finishes editing.

When nothing is being edited, **↑** and **↓** scroll, and **⌘↑** and **⌘↓** go to the top or bottom. **⌥↓**, **←** or **→** starts editing the first block on screen.

Clicking a link follows it. Links to Markdown files open in Kertas, and other links open in their own apps: web pages in your browser, for example. To edit a link, click elsewhere in its block, or move to it with **⌥↓** and **⌥↑**.

Clicking a task's checkbox (☐) ticks or unticks it. Click the task's text to edit it.

## Writing

- **↩** starts a new line. A single line break inside a paragraph shows as a space, as in all Markdown, so press **↩** twice to start a new paragraph.
- **⇧↩** adds a line break that shows, within a paragraph.
- In a list, **↩** starts the next item, with the next number or an empty task box. On an empty item, it ends the list. Quotes work the same way.
- **⇥** and **⇧⇥** indent and outdent list items, and code in code blocks.

See **Help → Keyboard Shortcuts** for every shortcut.

## Spelling

Misspelled words get a wavy underline a moment after you stop typing. Only prose is checked: code, web addresses, HTML and front matter aren't, and nor are words in capitals or with digits in them, which are usually names of things. **View → Check Spelling** turns it off and on. The language is **Spelling** in **Kertas → Settings…**: English (US) unless you choose another (see Settings).

## Finding and replacing

**Edit → Find (⌘F)** opens the find bar. As you type, every match is highlighted, and ⌘G and ⇧⌘G (or ↩ and ⇧↩) go from one to the next, selecting it. Find looks at the text as you see it, so "bold" finds `**bold**`. **Match Case** and **Whole Words** narrow the search. Esc closes the bar and leaves the match selected; ⌘E looks for the selected text.

**Edit → Find and Replace (⌥⌘F)** adds a replacement. **Replace** replaces the selected match and goes to the next; **Replace All** replaces every match, and one Undo takes it all back. Only the matched text changes, so the formatting around it stays.

With **Regex** on, the query is a regular expression, in the syntax of Rust's [regex](https://docs.rs/regex/latest/regex/#syntax) library, and the replacement can use its groups, as `$1` or `${name}`.

## Saving

**⌘S** saves, and **⇧⌘S** (Save As) saves under another name or in another folder. To save automatically, choose **After a pause** for **Autosave** in **Kertas → Settings…**. Documents then save a few seconds after you stop typing, when you switch to another tab, window or app, and when you close them or quit. A new document is only saved automatically once you've saved it the first time.

## When another app changes a file

If another app changes a document's file, such as another editor or `git checkout`, Kertas shows the new version straight away. **⌘Z** brings back what you had. If you have unsaved changes, Kertas asks first: **Reload** replaces your changes with the new file, and **Keep Mine** keeps them, so saving writes over it. If the file is deleted or moved, Kertas keeps the document open, and saving puts the file back.

## Tabs and windows

Each document opens in its own tab, in one window. **⌘1** to **⌘9** go straight to a tab, and **⇧⌘]** and **⇧⌘[** go to the next and previous one. Drag a tab along the strip to move it. **⌘W** closes a tab and **⇧⌘W** closes the window, asking first about each document with unsaved changes. To open documents in separate windows instead, choose **Windows** for "Open documents in" in **Kertas → Settings…**. That takes effect the next time Kertas starts.

## Printing

**File → Print…** (⌘P) prints the document as it looks rendered, in light colors. To save a PDF, use the PDF menu in the print panel.

## From the terminal

**Kertas → Install Command Line Tool…** installs the `kt` command. Then `kt notes.md` opens a file in Kertas. A file that doesn't exist yet opens as a new document, and saving creates it.

`kt --convert notes.md notes.pdf` saves a file as a PDF without opening it, laid out as **File → Print…** prints it with your settings.

## Appearance

**View → Appearance** chooses Light, Dark or Match System, and **View → Theme** chooses the colors. You can make your own themes too: see **Help → Themes**. **View → Text Width** chooses how wide the text is: Standard, Wide, or the whole window.

**⌘+** and **⌘-** make the text bigger or smaller in every document, and **⌘0** returns to your usual size. Zooming lasts until you quit. To change the size documents start at, use **Default text size** in **Kertas → Settings…**.

## Settings

Settings are kept in `~/Library/Application Support/kertas/config.toml`. **Kertas → Settings…** (⌘,) and the View menu change them for you, and you can also edit the file. Changes apply straight away, except `layout`, which applies the next time Kertas starts.

```toml
appearance = "dark"   # "light", "dark" or "system"
theme = "Kertas"      # the color theme
layout = "windows"    # "tabs" or "windows", from the next start
text_width = "wide"   # "standard", "wide" or "full"
font_size = 18        # 10 to 32
body_font = "Charter" # any installed font; the system font if not set
code_font = "SF Mono" # Menlo if not set
tab_width = 2         # how wide tab characters in code look: 2, 4 or 8 spaces
list_indent = "wide"  # how far nested list items are drawn in: "compact", "standard" or "wide"
editing_hint = false  # hide keyboard hints on the block being edited
scrollbar = false     # hide the scrollbar (also View → Show Scrollbar)
fade_other_blocks = true # fade every block but the one being edited (View → Fade Other Blocks)
typewriter_scrolling = true # keep the line being typed in the middle (View → Typewriter Scrolling)
autosave = 5          # save 5 seconds after typing stops; "off" if not set
spelling_language = "en-GB" # the spelling dictionary: "en-US" (the default), "en-GB" or one you add
check_spelling = false # don't underline misspelled words (View → Check Spelling)
```

If the file has a mistake, Kertas uses the default for that setting and shows what's wrong at the bottom of the window.

Kertas comes with English (US) and English (UK) spelling dictionaries. For another language, put a Hunspell dictionary's two files in the `dictionaries` folder next to `config.toml` (**Open Dictionaries Folder** in Settings opens it), named after the language, such as `fr.aff` and `fr.dic`, and choose it under **Spelling**. A language with a region, such as `fr-CA`, uses `fr` if there's no `fr-CA`. If there's no dictionary for the language, spelling isn't checked, and Kertas says so at the bottom of the window.

**Tab character width** only changes how tab characters in code look. It doesn't change what the Tab key inserts or how lists are indented. **List indent** only changes how far nested list items are drawn in; the Markdown keeps whatever indentation it has.

To make Kertas open Markdown files when you double-click them in Finder, choose **Kertas → Make Kertas the Default Markdown App**.

## Markdown

**Help → Markdown Syntax** shows all the Markdown Kertas supports, with examples.
