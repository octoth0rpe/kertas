//! Kami's settings (REQUIREMENTS §8): what they are, and reading and
//! writing them in the TOML settings file.
//!
//! The file is optional and every setting has a default (G2). Problems in
//! it never stop Kami: a bad value or an unknown setting falls back to the
//! default and is reported with its line (G4). Changing a setting edits
//! the file in place, keeping comments and formatting.

use std::fmt;
use std::ops::RangeInclusive;

use toml_edit::{DocumentMut, Item, Value};

use crate::tabs::{DEFAULT_TAB_WIDTH, TAB_WIDTHS};

/// The color scheme (A2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeSetting {
    Light,
    Dark,
    /// Follow the macOS appearance.
    #[default]
    System,
}

/// How documents are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    /// Each document is a tab in a shared window.
    #[default]
    Tabs,
    /// Each document has its own window.
    Windows,
}

/// How wide the text column is (A1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextWidth {
    #[default]
    Standard,
    Wide,
    /// The whole window, less its margins.
    Full,
}

/// The text sizes allowed, in pixels.
pub const FONT_SIZES: RangeInclusive<u32> = 10..=32;

/// The default text size, in pixels.
pub const DEFAULT_FONT_SIZE: u32 = 16;

/// All settings, with defaults for any the file doesn't set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeSetting,
    pub layout: Layout,
    pub text_width: TextWidth,
    /// The text size documents start at, in pixels.
    pub font_size: u32,
    /// How many spaces a tab is shown as.
    pub tab_width: u32,
    /// Whether the block being edited shows its keyboard hints.
    pub editing_hint: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeSetting::default(),
            layout: Layout::default(),
            text_width: TextWidth::default(),
            font_size: DEFAULT_FONT_SIZE,
            tab_width: DEFAULT_TAB_WIDTH,
            editing_hint: true,
        }
    }
}

/// A problem in the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The 1-based line, if the problem is at a particular place.
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// A setting: its name in the file, its label in the settings sheet, and
/// what values it takes. [`parse`], [`set`] and the settings sheet all
/// read [`SETTINGS`], so a setting is described once.
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    /// A note shown under it in the settings sheet. For a `Bool`, the
    /// checkbox's label.
    pub note: Option<&'static str>,
    pub kind: Kind,
}

/// The values a setting takes, and how to read and change it in
/// [`Settings`].
pub enum Kind {
    /// One of several strings. Each choice is its value in the file and
    /// its label in the settings sheet.
    Choice {
        choices: &'static [(&'static str, &'static str)],
        get: fn(&Settings) -> &'static str,
        apply: fn(&mut Settings, &str),
    },
    /// A whole number in a range.
    Integer {
        range: RangeInclusive<u32>,
        /// Shown after the number in the settings sheet, e.g. "px".
        unit: &'static str,
        get: fn(&Settings) -> u32,
        apply: fn(&mut Settings, u32),
    },
    /// One of several whole numbers.
    Numbers {
        choices: &'static [u32],
        get: fn(&Settings) -> u32,
        apply: fn(&mut Settings, u32),
    },
    /// `true` or `false`.
    Bool {
        get: fn(&Settings) -> bool,
        apply: fn(&mut Settings, bool),
    },
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "theme",
        label: "Appearance",
        note: None,
        kind: Kind::Choice {
            choices: &[
                ("light", "Light"),
                ("dark", "Dark"),
                ("system", "Match System"),
            ],
            get: |settings| settings.theme.as_str(),
            apply: |settings, value| {
                settings.theme = match value {
                    "light" => ThemeSetting::Light,
                    "dark" => ThemeSetting::Dark,
                    _ => ThemeSetting::System,
                }
            },
        },
    },
    Setting {
        key: "font_size",
        label: "Default text size",
        note: Some("Documents start at this size."),
        kind: Kind::Integer {
            range: FONT_SIZES,
            unit: "px",
            get: |settings| settings.font_size,
            apply: |settings, size| settings.font_size = size,
        },
    },
    Setting {
        key: "text_width",
        label: "Text width",
        note: None,
        kind: Kind::Choice {
            choices: &[
                ("standard", "Standard"),
                ("wide", "Wide"),
                ("full", "Full Width"),
            ],
            get: |settings| settings.text_width.as_str(),
            apply: |settings, value| {
                settings.text_width = match value {
                    "wide" => TextWidth::Wide,
                    "full" => TextWidth::Full,
                    _ => TextWidth::Standard,
                }
            },
        },
    },
    Setting {
        key: "tab_width",
        label: "Tab width",
        note: Some("Spaces shown for each tab."),
        kind: Kind::Numbers {
            choices: TAB_WIDTHS,
            get: |settings| settings.tab_width,
            apply: |settings, width| settings.tab_width = width,
        },
    },
    Setting {
        key: "editing_hint",
        label: "Editing hint",
        note: Some("Show keyboard hints on the block being edited"),
        kind: Kind::Bool {
            get: |settings| settings.editing_hint,
            apply: |settings, show| settings.editing_hint = show,
        },
    },
    Setting {
        key: "layout",
        label: "Open documents in",
        note: Some("Takes effect the next time Kami starts."),
        kind: Kind::Choice {
            choices: &[("tabs", "Tabs"), ("windows", "Windows")],
            get: |settings| settings.layout.as_str(),
            apply: |settings, value| {
                settings.layout = match value {
                    "windows" => Layout::Windows,
                    _ => Layout::Tabs,
                }
            },
        },
    },
];

/// The setting called `key`.
///
/// # Panics
///
/// If there's no such setting.
pub fn setting(key: &str) -> &'static Setting {
    find(key).unwrap_or_else(|| panic!("no setting `{key}`"))
}

fn find(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.key == key)
}

impl Kind {
    /// Reads `value` into `settings`, if it's allowed.
    fn read(&self, value: &Value, settings: &mut Settings) -> bool {
        match self {
            Self::Choice { choices, apply, .. } => match value.as_str() {
                Some(value) if choices.iter().any(|&(choice, _)| choice == value) => {
                    apply(settings, value);
                    true
                }
                _ => false,
            },
            Self::Integer { range, apply, .. } => {
                match value.as_integer().and_then(|n| u32::try_from(n).ok()) {
                    Some(n) if range.contains(&n) => {
                        apply(settings, n);
                        true
                    }
                    _ => false,
                }
            }
            Self::Numbers { choices, apply, .. } => {
                match value.as_integer().and_then(|n| u32::try_from(n).ok()) {
                    Some(n) if choices.contains(&n) => {
                        apply(settings, n);
                        true
                    }
                    _ => false,
                }
            }
            Self::Bool { apply, .. } => match value.as_bool() {
                Some(value) => {
                    apply(settings, value);
                    true
                }
                None => false,
            },
        }
    }

    /// What the setting must be, as in "`key` must be …".
    fn allowed(&self) -> String {
        match self {
            Self::Choice { choices, .. } => {
                let values: Vec<&str> = choices.iter().map(|&(value, _)| value).collect();
                one_of(&values)
            }
            Self::Integer { range, .. } => {
                format!("a whole number from {} to {}", range.start(), range.end())
            }
            Self::Numbers { choices, .. } => {
                let values: Vec<String> = choices.iter().map(u32::to_string).collect();
                one_of(&values.iter().map(String::as_str).collect::<Vec<_>>())
            }
            Self::Bool { .. } => one_of(&["true", "false"]),
        }
    }
}

impl ThemeSetting {
    /// The value written in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

impl TextWidth {
    /// The value written in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Wide => "wide",
            Self::Full => "full",
        }
    }
}

impl Layout {
    /// The value written in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tabs => "tabs",
            Self::Windows => "windows",
        }
    }
}

/// Reads settings from the file's `text`. Anything wrong is reported, and
/// the affected settings keep their defaults.
pub fn parse(text: &str) -> (Settings, Vec<Problem>) {
    let mut settings = Settings::default();
    let mut problems = Vec::new();
    let line_of = |offset: usize| text[..offset.min(text.len())].matches('\n').count() + 1;

    let document = match toml_edit::Document::parse(text) {
        Ok(document) => document,
        Err(err) => {
            problems.push(Problem {
                line: err.span().map(|span| line_of(span.start)),
                message: format!("{}; using the defaults", err.message().trim_end()),
            });
            return (settings, problems);
        }
    };
    for (key, item) in document.iter() {
        let line = document
            .get_key_value(key)
            .and_then(|(key, _)| key.span())
            .map(|span| line_of(span.start));
        let Some(setting) = find(key) else {
            problems.push(Problem {
                line,
                message: format!("unknown setting `{key}`"),
            });
            continue;
        };
        let read = item
            .as_value()
            .is_some_and(|value| setting.kind.read(value, &mut settings));
        if !read {
            problems.push(Problem {
                line,
                message: format!(
                    "`{key}` must be {}; using the default",
                    setting.kind.allowed()
                ),
            });
        }
    }
    (settings, problems)
}

/// The file's `text` with `key` set to `value`, keeping everything else,
/// including comments. Fails if the file can't be parsed, so it's never
/// overwritten, or if `value` isn't allowed for `key`.
pub fn set(text: &str, key: &str, value: impl Into<Value>) -> Result<String, Problem> {
    let value = value.into();
    let allowed = find(key).map(|setting| {
        let mut scratch = Settings::default();
        (setting.kind.read(&value, &mut scratch), setting)
    });
    match allowed {
        None => {
            return Err(Problem {
                line: None,
                message: format!("unknown setting `{key}`"),
            });
        }
        Some((false, setting)) => {
            return Err(Problem {
                line: None,
                message: format!("`{key}` must be {}", setting.kind.allowed()),
            });
        }
        Some((true, _)) => {}
    }
    let mut document: DocumentMut = text.parse().map_err(|err: toml_edit::TomlError| Problem {
        line: None,
        message: format!(
            "couldn't change `{key}` because the settings file has a problem: {}",
            err.message().trim_end()
        ),
    })?;
    let has_tables = document.iter().any(|(_, item)| item.is_table_like());
    match document.get_mut(key) {
        // Replace just the value, keeping its comments and spacing.
        Some(Item::Value(existing)) => {
            let decor = existing.decor().clone();
            *existing = value;
            *existing.decor_mut() = decor;
        }
        _ if !has_tables => {
            // Add it at the end, after any comments there, which toml_edit
            // would otherwise keep after it.
            let mut text = text.to_owned();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let mut line = DocumentMut::new();
            line[key] = Item::Value(value);
            text.push_str(&line.to_string());
            return Ok(text);
        }
        _ => document[key] = Item::Value(value),
    }
    Ok(document.to_string())
}

/// "`a`, `b` or `c`".
fn one_of(values: &[&str]) -> String {
    let quoted: Vec<String> = values.iter().map(|value| format!("`{value}`")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => quoted.concat(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(parse(""), (Settings::default(), vec![]));
        assert_eq!(Settings::default().theme, ThemeSetting::System);
        assert_eq!(Settings::default().layout, Layout::Tabs);
    }

    #[test]
    fn reads_settings() {
        let (settings, problems) =
            parse("# Mine\ntheme = \"dark\"\nlayout = \"windows\"\ntext_width = \"wide\"\n");
        assert_eq!(problems, vec![]);
        assert_eq!(
            settings,
            Settings {
                theme: ThemeSetting::Dark,
                layout: Layout::Windows,
                text_width: TextWidth::Wide,
                font_size: DEFAULT_FONT_SIZE,
                tab_width: DEFAULT_TAB_WIDTH,
                editing_hint: true,
            }
        );
    }

    #[test]
    fn bad_values_fall_back_with_their_line() {
        let (settings, problems) = parse("layout = \"windows\"\n\ntheme = \"blue\"\n");
        assert_eq!(settings.theme, ThemeSetting::System);
        assert_eq!(
            settings.layout,
            Layout::Windows,
            "good settings still apply"
        );
        assert_eq!(
            problems,
            vec![Problem {
                line: Some(3),
                message: "`theme` must be `light`, `dark` or `system`; using the default".into(),
            }]
        );
        let (_, problems) = parse("theme = 3\n");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn unknown_settings_are_reported() {
        let (settings, problems) = parse("theme = \"light\"\nfont = \"x\"\n");
        assert_eq!(settings.theme, ThemeSetting::Light);
        assert_eq!(problems[0].to_string(), "line 2: unknown setting `font`");
    }

    #[test]
    fn invalid_toml_uses_defaults() {
        let (settings, problems) = parse("theme = \"dark\"\ntheme = = \n");
        assert_eq!(settings, Settings::default());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn set_keeps_comments_and_formatting() {
        let text = "# My settings\ntheme = \"dark\"  # for night work\n\nlayout = \"tabs\"\n";
        assert_eq!(
            set(text, "theme", "light").unwrap(),
            "# My settings\ntheme = \"light\"  # for night work\n\nlayout = \"tabs\"\n"
        );
    }

    #[test]
    fn set_adds_missing_settings() {
        assert_eq!(set("", "theme", "dark").unwrap(), "theme = \"dark\"\n");
        assert_eq!(
            set("# Mine\n", "layout", "windows").unwrap(),
            "# Mine\nlayout = \"windows\"\n"
        );
    }

    #[test]
    fn font_size() {
        assert_eq!(parse("font_size = 20\n").0.font_size, 20);
        let (settings, problems) = parse("font_size = 99\n");
        assert_eq!(settings.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(problems.len(), 1);
        assert_eq!(parse("font_size = \"big\"\n").1.len(), 1);
        assert_eq!(set("", "font_size", 18).unwrap(), "font_size = 18\n");
        assert_eq!(
            set("font_size = 18  # big\n", "font_size", 20).unwrap(),
            "font_size = 20  # big\n"
        );
    }

    #[test]
    fn editing_hint() {
        assert!(Settings::default().editing_hint);
        assert!(!parse("editing_hint = false\n").0.editing_hint);
        let (settings, problems) = parse("editing_hint = \"no\"\n");
        assert!(settings.editing_hint);
        assert_eq!(
            problems[0].to_string(),
            "line 1: `editing_hint` must be `true` or `false`; using the default"
        );
        assert_eq!(
            set("", "editing_hint", false).unwrap(),
            "editing_hint = false\n"
        );
        assert_eq!(
            set("editing_hint = false # quiet\n", "editing_hint", true).unwrap(),
            "editing_hint = true # quiet\n"
        );
    }

    #[test]
    fn tab_width() {
        assert_eq!(Settings::default().tab_width, 4);
        assert_eq!(parse("tab_width = 8\n").0.tab_width, 8);
        let (settings, problems) = parse("tab_width = 3\n");
        assert_eq!(settings.tab_width, 4);
        assert_eq!(
            problems[0].to_string(),
            "line 1: `tab_width` must be `2`, `4` or `8`; using the default"
        );
        assert!(set("", "tab_width", 3).is_err());
    }

    #[test]
    fn integer_edge_cases() {
        for text in ["font_size = 16.0\n", "font_size = -1\n", "font_size = 9\n"] {
            let (settings, problems) = parse(text);
            assert_eq!(settings.font_size, DEFAULT_FONT_SIZE, "{text}");
            assert_eq!(problems.len(), 1, "{text}");
        }
        assert_eq!(parse("font_size = 32\n").0.font_size, 32);
    }

    #[test]
    fn tables_are_reported_and_kept() {
        let text = "theme = \"dark\"\n\n[extra]\nx = 1\n";
        let (settings, problems) = parse(text);
        assert_eq!(settings.theme, ThemeSetting::Dark);
        assert_eq!(problems[0].to_string(), "line 3: unknown setting `extra`");
        // A new setting goes before the table, so it isn't inside it.
        let changed = set(text, "layout", "windows").unwrap();
        assert_eq!(
            changed,
            "theme = \"dark\"\nlayout = \"windows\"\n\n[extra]\nx = 1\n"
        );
        assert_eq!(parse(&changed).0.layout, Layout::Windows);
        // A setting written as a table is replaced by a value.
        let changed = set("[theme]\nx = 1\n", "theme", "light").unwrap();
        assert_eq!(
            parse(&changed),
            (
                Settings {
                    theme: ThemeSetting::Light,
                    ..Settings::default()
                },
                vec![]
            )
        );
    }

    #[test]
    fn line_numbers_with_crlf_and_duplicates() {
        let (_, problems) = parse("theme = \"dark\"\r\n\r\nlayout = 1\r\n");
        assert_eq!(problems[0].line, Some(3));
        let (settings, problems) = parse("theme = \"dark\"\ntheme = \"light\"\n");
        assert_eq!(settings, Settings::default());
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn set_refuses_bad_values() {
        assert!(set("", "theme", "blue").is_err());
        assert!(set("", "font_size", 99).is_err());
        assert!(set("", "editing_hint", "yes").is_err());
        assert!(set("", "nonsense", true).is_err());
    }

    #[test]
    fn every_setting_round_trips() {
        for setting in SETTINGS {
            match &setting.kind {
                Kind::Choice { choices, get, .. } => {
                    for &(value, _) in *choices {
                        let text = set("", setting.key, value).unwrap();
                        assert_eq!(get(&parse(&text).0), value, "{}", setting.key);
                    }
                }
                Kind::Integer { range, get, .. } => {
                    for n in [*range.start(), *range.end()] {
                        let text = set("", setting.key, i64::from(n)).unwrap();
                        assert_eq!(get(&parse(&text).0), n, "{}", setting.key);
                    }
                }
                Kind::Numbers { choices, get, .. } => {
                    for &n in *choices {
                        let text = set("", setting.key, i64::from(n)).unwrap();
                        assert_eq!(get(&parse(&text).0), n, "{}", setting.key);
                    }
                }
                Kind::Bool { get, .. } => {
                    for b in [true, false] {
                        let text = set("", setting.key, b).unwrap();
                        assert_eq!(get(&parse(&text).0), b, "{}", setting.key);
                    }
                }
            }
        }
    }

    #[test]
    fn set_refuses_invalid_files() {
        assert!(set("theme = = ", "theme", "dark").is_err());
    }
}
