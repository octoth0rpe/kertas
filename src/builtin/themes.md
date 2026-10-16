# Themes

A theme sets the colors Kertas uses, in light and dark mode. **View → Theme** or **Theme** in **Kertas → Settings…** chooses one, and **View → Appearance** chooses whether its light or dark colors are shown.

## Making your own

Your themes are files in the `themes` folder next to the settings file, `~/Library/Application Support/kertas/themes`. **Open Themes Folder** in **Kertas → Settings…** opens it in Finder, creating it if needed.

Each theme is one `.toml` file, with any file name. It names the theme and gives its colors for light and dark mode. Your themes are listed after Kertas's own, and adding, changing or removing a file applies straight away.

The easiest way to start is to copy the example below into a new file in the folder, change its `name`, and change the colors you want.

A theme with the same name as one of Kertas's, such as `name = "Solarized"`, takes its place. That lets you tweak one of Kertas's themes without starting from scratch.

If a theme file has a mistake, Kertas shows what's wrong at the bottom of the window, naming the file. A color that's missing or wrong uses Kertas's own color instead, and so does a theme without `[light]` or `[dark]`, so a theme for dark mode only still works. Only a file without a `name` is left out.

## Colors

Colors are written as `"#rrggbb"`, for example `"#0969da"`. Add two more digits for opacity: `"#0969da40"` is a quarter opaque, so what's beneath shows through. Selections and the edited block's marking usually need some transparency.

| Key | What it colors |
|-----|----------------|
| `background` | The window behind the document |
| `text` | Text |
| `muted_text` | Quotes, the smallest headings, the dimmed Markdown characters in the block being edited, and hints |
| `link` | Links |
| `code_background` | Behind code and code blocks, and behind buttons in Settings |
| `rule` | Horizontal rules, the bar beside quotes, table borders, and the lines between tabs |
| `table_header_background` | Table headers, the tab bar, and the bars at the top and bottom of the window |
| `selection` | Behind selected text |
| `caret` | The caret, and the accent color of buttons and checkboxes |
| `accent_text` | Text on the accent color, such as the Done button's label |
| `revealed_background` | Behind the block being edited |
| `revealed_border` | The border around the block being edited |
| `alert_note` | Note alerts (`> [!NOTE]`) |
| `alert_tip` | Tip alerts (`> [!TIP]`) |
| `alert_important` | Important alerts (`> [!IMPORTANT]`) |
| `alert_warning` | Warning alerts (`> [!WARNING]`) |
| `alert_caution` | Caution alerts (`> [!CAUTION]`) |

Make sure text stays easy to read: Kertas's own themes keep `text`, `muted_text`, `link` and the alert colors at a contrast of at least 4.5 to 1 against `background`.

## Example

This is Kertas's own theme, with every color, renamed so it doesn't replace it. Lines starting with `#` are comments.

```toml
{example}
```
