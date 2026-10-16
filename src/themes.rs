//! Color themes (A2): reading theme files, and the themes built into
//! Kertas.
//!
//! A theme is a pair: colors for light mode and colors for dark mode. The
//! appearance setting picks which are shown. A theme file names the theme
//! and gives both:
//!
//! ```toml
//! name = "Kertas"
//!
//! [light]
//! background = "#ffffff"
//! text = "#1f2328"
//!
//! [dark]
//! background = "#16181d"
//! text = "#e6e8eb"
//! ```
//!
//! Like the settings file, problems in it never stop Kertas: a missing or
//! bad color, or a missing half, falls back to Kertas's colors and is
//! reported with its line. Only a theme without a usable name is rejected.

use std::sync::OnceLock;

use toml_edit::{Document, Item, Table};

use crate::settings::Problem;

/// A color, as `0xRRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    /// Reads `"#rrggbb"` or `"#rrggbbaa"`.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        match hex.len() {
            6 => Some(Self(value << 8 | 0xff)),
            8 => Some(Self(value)),
            _ => None,
        }
    }
}

/// Light or dark mode: which half of a theme is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    /// The table a theme file gives its colors in.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// A theme's colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color,
    pub text: Color,
    /// Secondary text: blockquotes, h6, source shown as-is, hints.
    pub muted_text: Color,
    pub link: Color,
    pub code_background: Color,
    /// Thematic breaks, blockquote bars and table borders.
    pub rule: Color,
    pub table_header_background: Color,
    /// Background of selected text.
    pub selection: Color,
    /// The caret, and the accent color of controls.
    pub caret: Color,
    /// Text drawn on the accent color, e.g. a default button's label.
    pub accent_text: Color,
    /// Background and border that mark the revealed block.
    pub revealed_background: Color,
    pub revealed_border: Color,
    /// GitHub alerts' colors.
    pub alert_note: Color,
    pub alert_tip: Color,
    pub alert_important: Color,
    pub alert_warning: Color,
    pub alert_caution: Color,
}

/// Where a color goes in a [`Palette`].
pub type Field = fn(&mut Palette) -> &mut Color;

/// Each color's key in a theme file, and where it goes in a [`Palette`].
pub const COLORS: &[(&str, Field)] = &[
    ("background", |palette| &mut palette.background),
    ("text", |palette| &mut palette.text),
    ("muted_text", |palette| &mut palette.muted_text),
    ("link", |palette| &mut palette.link),
    ("code_background", |palette| &mut palette.code_background),
    ("rule", |palette| &mut palette.rule),
    ("table_header_background", |palette| {
        &mut palette.table_header_background
    }),
    ("selection", |palette| &mut palette.selection),
    ("caret", |palette| &mut palette.caret),
    ("accent_text", |palette| &mut palette.accent_text),
    ("revealed_background", |palette| {
        &mut palette.revealed_background
    }),
    ("revealed_border", |palette| &mut palette.revealed_border),
    ("alert_note", |palette| &mut palette.alert_note),
    ("alert_tip", |palette| &mut palette.alert_tip),
    ("alert_important", |palette| &mut palette.alert_important),
    ("alert_warning", |palette| &mut palette.alert_warning),
    ("alert_caution", |palette| &mut palette.alert_caution),
];

/// A theme: its name, and its colors in light and dark mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub light: Palette,
    pub dark: Palette,
}

impl Theme {
    /// The colors for light or dark mode.
    pub fn palette(&self, appearance: Appearance) -> &Palette {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }
}

const KERTAS: &str = include_str!("builtin/themes/kertas.toml");

/// The theme files built into Kertas, in the order they're listed.
const BUILTIN: &[&str] = &[
    KERTAS,
    include_str!("builtin/themes/solarized.toml"),
    include_str!("builtin/themes/gruvbox.toml"),
    include_str!("builtin/themes/catppuccin.toml"),
];

/// The themes built into Kertas.
pub fn builtin() -> &'static [Theme] {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    THEMES.get_or_init(|| {
        BUILTIN
            .iter()
            .map(|text| parse(text).expect("built-in themes are valid").0)
            .collect()
    })
}

/// The name of the theme in `themes` called `name`, ignoring case, as
/// it's spelled. For [`ThemeCheck`].
///
/// [`ThemeCheck`]: crate::settings::ThemeCheck
pub fn find(themes: &[Theme], name: &str) -> Option<String> {
    themes
        .iter()
        .find(|theme| theme.name.eq_ignore_ascii_case(name.trim()))
        .map(|theme| theme.name.clone())
}

/// Kertas's own theme: the default, whose colors stand in for any a theme
/// file is missing.
pub fn kertas() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    // It gives every color, so nothing falls back to it.
    THEME.get_or_init(|| {
        read(KERTAS, &|_| Palette::BLACK)
            .expect("Kertas's theme is valid")
            .0
    })
}

impl Palette {
    /// Only a starting point for Kertas's own theme, which sets every
    /// color.
    const BLACK: Self = Self {
        background: Color(0xff),
        text: Color(0xff),
        muted_text: Color(0xff),
        link: Color(0xff),
        code_background: Color(0xff),
        rule: Color(0xff),
        table_header_background: Color(0xff),
        selection: Color(0xff),
        caret: Color(0xff),
        accent_text: Color(0xff),
        revealed_background: Color(0xff),
        revealed_border: Color(0xff),
        alert_note: Color(0xff),
        alert_tip: Color(0xff),
        alert_important: Color(0xff),
        alert_warning: Color(0xff),
        alert_caution: Color(0xff),
    };
}

/// Reads a theme from a theme file's `text`. Missing or bad colors, and a
/// missing half, are reported and take Kertas's. Fails, with the reasons,
/// if the file can't be read or has no usable `name`.
pub fn parse(text: &str) -> Result<(Theme, Vec<Problem>), Vec<Problem>> {
    read(text, &|appearance| *kertas().palette(appearance))
}

/// Reads a theme like [`parse`], taking colors the file doesn't give
/// from `fallback`.
fn read(
    text: &str,
    fallback: &dyn Fn(Appearance) -> Palette,
) -> Result<(Theme, Vec<Problem>), Vec<Problem>> {
    let line_of = |offset: usize| text[..offset.min(text.len())].matches('\n').count() + 1;
    let document = Document::parse(text).map_err(|err| {
        vec![Problem {
            line: err.span().map(|span| line_of(span.start)),
            message: err.message().trim_end().to_owned(),
        }]
    })?;
    let file = File {
        root: document.as_table(),
        line_of: &line_of,
    };

    let mut problems = Vec::new();
    let mut name = None;
    for (key, item) in file.root.iter() {
        match key {
            "name" => match item.as_str().map(str::trim) {
                Some(value) if !value.is_empty() => name = Some(value.to_owned()),
                _ => problems.push(file.problem(
                    file.root,
                    key,
                    "`name` must be a name in quotes".into(),
                )),
            },
            "light" | "dark" => {}
            _ => problems.push(file.problem(file.root, key, format!("unknown key `{key}`"))),
        }
    }
    if !file.root.contains_key("name") {
        problems.push(Problem {
            line: None,
            message: "`name` is missing".into(),
        });
    }
    let Some(name) = name else {
        return Err(problems);
    };
    let light = file.palette(Appearance::Light, fallback, &mut problems);
    let dark = file.palette(Appearance::Dark, fallback, &mut problems);
    Ok((Theme { name, light, dark }, problems))
}

/// A parsed theme file, for reporting problems at their lines.
struct File<'a> {
    root: &'a Table,
    line_of: &'a dyn Fn(usize) -> usize,
}

impl File<'_> {
    /// A problem with `key` in `table`, at its line.
    fn problem(&self, table: &Table, key: &str, message: String) -> Problem {
        let line = table
            .get_key_value(key)
            .and_then(|(key, _)| key.span())
            .map(|span| (self.line_of)(span.start));
        Problem { line, message }
    }

    /// The colors for light or dark mode, taking any that are missing or
    /// bad from `fallback`.
    fn palette(
        &self,
        appearance: Appearance,
        fallback: &dyn Fn(Appearance) -> Palette,
        problems: &mut Vec<Problem>,
    ) -> Palette {
        let mut palette = fallback(appearance);
        let half = appearance.as_str();
        let table = match self.root.get(half) {
            Some(Item::Table(table)) => table,
            None => {
                problems.push(Problem {
                    line: None,
                    message: format!("`[{half}]` is missing; using Kertas's {half} colors"),
                });
                return palette;
            }
            Some(_) => {
                problems.push(self.problem(
                    self.root,
                    half,
                    format!("`{half}` must be a table; using Kertas's {half} colors"),
                ));
                return palette;
            }
        };
        for &(key, field) in COLORS {
            match table.get(key) {
                None => problems.push(Problem {
                    line: None,
                    message: format!("`{half}.{key}` is missing; using Kertas's"),
                }),
                Some(item) => match item.as_str().and_then(Color::parse) {
                    Some(color) => *field(&mut palette) = color,
                    None => problems.push(self.problem(
                        table,
                        key,
                        format!(
                            "`{half}.{key}` must be a color like \"#0969da\" or \
                             \"#0969da40\"; using Kertas's"
                        ),
                    )),
                },
            }
        }
        for (key, _) in table.iter() {
            if !COLORS.iter().any(|&(name, _)| name == key) {
                problems.push(self.problem(table, key, format!("unknown color `{half}.{key}`")));
            }
        }
        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(text: &str) -> Vec<String> {
        match parse(text) {
            Ok((_, problems)) | Err(problems) => problems.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn reads_colors() {
        assert_eq!(Color::parse("#0969da"), Some(Color(0x0969daff)));
        assert_eq!(Color::parse("#0969DA40"), Some(Color(0x0969da40)));
        for bad in ["0969da", "#0969d", "#0969da4", "#+969da", "#gggggg", ""] {
            assert_eq!(Color::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn builtin_themes_are_complete() {
        for text in BUILTIN {
            let (theme, problems) = parse(text).unwrap();
            assert_eq!(problems, vec![], "{}", theme.name);
        }
        let names: Vec<_> = builtin().iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names, ["Kertas", "Solarized", "Gruvbox", "Catppuccin"]);
        assert_ne!(kertas().light, kertas().dark);
    }

    #[test]
    fn missing_and_bad_colors_fall_back() {
        let text = "name = \"Mine\"\n\n[light]\n\n[dark]\n\
                    background = \"#000000\"\ntext = \"white\"\n";
        let (theme, problems) = parse(text).unwrap();
        let kertas = kertas();
        assert_eq!(theme.name, "Mine");
        assert_eq!(theme.light, kertas.light);
        assert_eq!(theme.dark.background, Color(0xff));
        assert_eq!(theme.dark.text, kertas.dark.text);
        assert_eq!(theme.dark.link, kertas.dark.link);
        let problems: Vec<_> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(problems.len(), 2 * COLORS.len() - 1);
        assert_eq!(problems[0], "`light.background` is missing; using Kertas's");
        assert_eq!(
            problems[COLORS.len()],
            "line 7: `dark.text` must be a color like \"#0969da\" or \"#0969da40\"; \
             using Kertas's"
        );
    }

    #[test]
    fn a_missing_half_is_kertas() {
        let light = KERTAS
            .split("[dark]")
            .next()
            .unwrap()
            .replace("Kertas", "Day");
        let (theme, reported) = parse(&light).unwrap();
        assert_eq!(theme.name, "Day");
        assert_eq!(theme.dark, kertas().dark);
        assert_eq!(
            reported.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["`[dark]` is missing; using Kertas's dark colors"]
        );
        assert_eq!(
            problems("name = \"Odd\"\nlight = 3\n[dark]\n")[0],
            "line 2: `light` must be a table; using Kertas's light colors"
        );
    }

    #[test]
    fn reports_unknown_keys() {
        let text = format!("{KERTAS}hue = \"#000000\"\n");
        assert_eq!(problems(&text), ["line 41: unknown color `dark.hue`"]);
        let text = format!("font = \"Charter\"\n{KERTAS}");
        assert_eq!(problems(&text), ["line 1: unknown key `font`"]);
    }

    #[test]
    fn rejects_themes_without_a_name() {
        assert_eq!(problems("[light]\n[dark]\n"), ["`name` is missing"]);
        assert_eq!(
            problems("name = \" \"\n"),
            ["line 1: `name` must be a name in quotes"]
        );
        assert!(parse("name = \" \"\n").is_err());
        assert_eq!(problems("name = = \n").len(), 1);
    }
}
