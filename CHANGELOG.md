# Release Notes

What's new in each version of Kertas, newest first.

## 0.5.0 — 2026-10-25

- **Spell checking.** Misspelled words get a wavy underline a moment after you stop typing. Only prose is checked, not code, web addresses or front matter. Right-click a word for suggestions, **Ignore**, **Add for This Document** or **Add to Dictionary**, and **View → Check Spelling** turns it off and on.
- **Spelling** in **Kertas → Settings…** chooses the language: English (US) or English (UK), or another language from a Hunspell dictionary you put in the dictionaries folder (**Open Dictionaries Folder**). A language without a region, such as `en`, uses one of its regions' dictionaries.
- **Edit → Find (⌘F)** finds text in the document. Every match is highlighted, ⌘G and ⇧⌘G go from one to the next, and **Match Case** and **Whole Words** narrow the search. It finds the text as you see it, so **bold** finds "bold" in `**bold**`.
- **Edit → Find and Replace (⌥⌘F)** replaces matches one at a time, or all at once with **Replace All**, which one Undo takes back. Only the matched text changes: the formatting around it, and every other byte of the file, stay as they were.
- **Regex** in the find bar searches with a regular expression, and a replacement can use its groups, as `$1` or `${name}`. A pattern with a mistake says what's wrong in the bar.
- **`fade_other_blocks = true`** in the settings file draws every block but the one you're editing faded, to keep your eye on what you're writing. Faded text stays readable in every theme, so some themes fade less than others. It's off unless you turn it on.
- **View → Typewriter Scrolling** keeps the line you're typing on in the middle of the window, scrolling the text up as you write. You can still scroll away, and the next keystroke brings it back.
- **Tabs are drawn by Kertas**, all in one window. **⇧⌘]** and **⇧⌘[** go to the next and previous tab, you can drag a tab along the strip to move it, and **⇧⌘W** closes the window, asking about each document with unsaved changes.
- The active tab is underlined in the theme's accent color, so it's easier to tell from the others.
- An empty file opens with the caret in it, ready for typing.
- Printed images are shrunk to fit the page's height as well as its width.
- A change to the settings file or a theme made just after another one is no longer sometimes missed.

## 0.4.0 — 2026-10-16

- **Color themes.** Choose Kertas, Solarized, Gruvbox or Catppuccin from **View → Theme** or **Kertas → Settings…**. Each has light and dark colors, and **View → Appearance** chooses which you see.
- **Your own themes.** Theme files in the themes folder (**Open Themes Folder** in Settings) are listed with the built-in ones, and changes to them show as soon as you save. A theme with a built-in theme's name replaces it. **Help → Themes** lists every color, with an example to start from.
- The `theme` setting now names the color theme, and light, dark or system is now the `appearance` setting. Settings files from before keep working.
- **Help → Release Notes** shows what's new in each version.

## 0.3.0 — 2026-10-16

- **About Kertas** tells where the name comes from: *kertas* is the Indonesian and Malay word for paper.
- The command line tool is now called `kt`.
- GitHub alerts: quotes starting with `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` or `[!CAUTION]` show as colored callouts.
- Footnotes.
- Links to headings, such as `[Usage](#usage)`, and to HTML anchors go to them.
- Images in Markdown are shown. Images from the web load only after you allow it, for each document.
- Bare web addresses, such as https://example.com, become links.
- `kt --convert notes.md notes.pdf` saves a Markdown file as a PDF without opening it.
- A scrollbar, which **View → Show Scrollbar** hides.
- Tabs for files with the same name show part of their folder, such as `README.md — docs`.
- With nothing being edited, the arrow keys scroll the document, smoothly when held, instead of jumping to its start.
- Moving between blocks with **⌥↓** and **⌥↑** scrolls to keep the caret in view.
- Going to a heading, a footnote or the caret no longer flickers.
- **Help → Markdown Syntax** shows all the Markdown Kertas understands.

## 0.2.0 — 2026-10-13

- Clicking a link opens it: Markdown files in Kertas, and anything else in its own app.
- Clicking a task's checkbox ticks or unticks it.
- Autosave: documents can save a few seconds after you stop typing.
- When another app changes an open document's file, Kertas shows the new version, and asks first if you have unsaved changes.
- New settings for the text and code fonts, how wide tab characters in code look, how far nested lists are drawn in, and hiding the editing hint.
- A shorter Settings window. Making Kertas the default Markdown app and installing the command line tool moved to the Kertas menu.
- Better VoiceOver and keyboard support: Settings, tabs and headings are labelled, and Tab moves between the controls in Settings.

## 0.1.0 — 2026-10-13

The first release.

- Open, edit and save Markdown files. Everything is shown rendered, except the block you're editing, which shows its Markdown. Files keep their formatting exactly, apart from what you change.
- Headings, paragraphs, lists, quotes, code blocks and tables.
- **↩** continues lists, **⇧↩** adds a line break, and **⇥** and **⇧⇥** indent and outdent list items.
- Light and dark appearance, following macOS unless you choose (**View → Appearance**).
- Text width and zoom, from the View menu.
- Documents open as tabs, and **⌘1** to **⌘9** switch between them.
- Printing.
- A settings file, and a Settings window (**⌘,**). Changes apply straight away.
- A command line tool opens files from the terminal.
- Kertas can be made the default app for Markdown files.
- **Help → Kertas Help** and **Help → Keyboard Shortcuts**, and **About Kertas**.
