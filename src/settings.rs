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

use crate::rendered::CODE_FONT;
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

/// How far nested list items are drawn in from their parent, on screen
/// and in print. The Markdown keeps whatever indentation it has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListIndent {
    Compact,
    #[default]
    Standard,
    Wide,
}

impl ListIndent {
    /// The indent relative to `Standard`.
    pub fn scale(self) -> f32 {
        match self {
            Self::Compact => 0.75,
            Self::Standard => 1.,
            Self::Wide => 1.5,
        }
    }

    /// The value written in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Wide => "wide",
        }
    }
}

/// The text sizes allowed, in pixels.
pub const FONT_SIZES: RangeInclusive<u32> = 10..=32;

/// The default text size, in pixels.
pub const DEFAULT_FONT_SIZE: u32 = 16;

/// The pauses after which autosave can save, in seconds.
pub const AUTOSAVE_SECONDS: RangeInclusive<u32> = 1..=300;

/// The pause the Settings sheet chooses when turning autosave on, in
/// seconds.
pub const DEFAULT_AUTOSAVE_SECONDS: u32 = 5;

/// All settings, with defaults for any the file doesn't set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeSetting,
    pub layout: Layout,
    pub text_width: TextWidth,
    /// The text size documents start at, in pixels.
    pub font_size: u32,
    /// How many spaces a tab is shown as.
    pub tab_width: u32,
    /// How far nested list items are indented.
    pub list_indent: ListIndent,
    /// The font family for text, or `None` for the system font.
    pub body_font: Option<String>,
    /// The font family for code, or `None` for [`CODE_FONT`].
    pub code_font: Option<String>,
    /// Whether the block being edited shows its keyboard hints.
    pub editing_hint: bool,
    /// Save documents once typing has paused this many seconds, or
    /// `None` not to save them automatically (F6).
    pub autosave: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeSetting::default(),
            layout: Layout::default(),
            text_width: TextWidth::default(),
            font_size: DEFAULT_FONT_SIZE,
            tab_width: DEFAULT_TAB_WIDTH,
            list_indent: ListIndent::default(),
            body_font: None,
            code_font: None,
            editing_hint: true,
            autosave: None,
        }
    }
}

impl Settings {
    /// The font family for code.
    pub fn code_font(&self) -> &str {
        self.code_font.as_deref().unwrap_or(CODE_FONT)
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
    /// Its label in the settings sheet. Fonts share a row labelled
    /// "Fonts", so theirs say which font.
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
    /// A font family name, or the default when not set.
    Font {
        /// The default's name in the settings sheet.
        default: &'static str,
        get: fn(&Settings) -> Option<&str>,
        apply: fn(&mut Settings, Option<String>),
    },
    /// `true` or `false`.
    Bool {
        get: fn(&Settings) -> bool,
        apply: fn(&mut Settings, bool),
    },
    /// `"off"`, or a whole number in a range.
    OffOrInteger {
        range: RangeInclusive<u32>,
        /// The number the settings sheet sets when turning it on.
        on: u32,
        get: fn(&Settings) -> Option<u32>,
        apply: fn(&mut Settings, Option<u32>),
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
        note: None,
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
        key: "body_font",
        label: "Text",
        note: None,
        kind: Kind::Font {
            default: "System",
            get: |settings| settings.body_font.as_deref(),
            apply: |settings, font| settings.body_font = font,
        },
    },
    Setting {
        key: "code_font",
        label: "Code",
        note: None,
        kind: Kind::Font {
            default: "Menlo (default)",
            get: |settings| settings.code_font.as_deref(),
            apply: |settings, font| settings.code_font = font,
        },
    },
    Setting {
        key: "tab_width",
        label: "Tab character width",
        note: Some("How wide tab characters in code look."),
        kind: Kind::Numbers {
            choices: TAB_WIDTHS,
            get: |settings| settings.tab_width,
            apply: |settings, width| settings.tab_width = width,
        },
    },
    Setting {
        key: "list_indent",
        label: "List indent",
        note: Some("Doesn't change the Markdown."),
        kind: Kind::Choice {
            choices: &[
                ("compact", "Compact"),
                ("standard", "Standard"),
                ("wide", "Wide"),
            ],
            get: |settings| settings.list_indent.as_str(),
            apply: |settings, value| {
                settings.list_indent = match value {
                    "compact" => ListIndent::Compact,
                    "wide" => ListIndent::Wide,
                    _ => ListIndent::Standard,
                }
            },
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
        key: "autosave",
        label: "Autosave",
        note: None,
        kind: Kind::OffOrInteger {
            range: AUTOSAVE_SECONDS,
            on: DEFAULT_AUTOSAVE_SECONDS,
            get: |settings| settings.autosave,
            apply: |settings, seconds| settings.autosave = seconds,
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
    /// Reads `value` into `settings`, or says what's wrong with it, as in
    /// "`key` …". `installed` gives a font family's name as installed, or
    /// `None` if it isn't.
    fn read(
        &self,
        value: &Value,
        settings: &mut Settings,
        installed: &FontCheck,
    ) -> Result<(), String> {
        let invalid = || Err(format!("must be {}", self.allowed()));
        let number = value.as_integer().and_then(|n| u32::try_from(n).ok());
        match self {
            Self::Choice { choices, apply, .. } => match value.as_str() {
                Some(value) if choices.iter().any(|&(choice, _)| choice == value) => {
                    apply(settings, value)
                }
                _ => return invalid(),
            },
            Self::Integer { range, apply, .. } => match number {
                Some(n) if range.contains(&n) => apply(settings, n),
                _ => return invalid(),
            },
            Self::Numbers { choices, apply, .. } => match number {
                Some(n) if choices.contains(&n) => apply(settings, n),
                _ => return invalid(),
            },
            Self::Font { apply, .. } => match value.as_str() {
                Some(name) => match installed(name) {
                    Some(name) => apply(settings, Some(name)),
                    None => return Err(format!("names \"{name}\", which isn't installed")),
                },
                None => return invalid(),
            },
            Self::Bool { apply, .. } => match value.as_bool() {
                Some(value) => apply(settings, value),
                None => return invalid(),
            },
            Self::OffOrInteger { range, apply, .. } => match (value.as_str(), number) {
                (Some("off"), _) => apply(settings, None),
                (_, Some(n)) if range.contains(&n) => apply(settings, Some(n)),
                _ => return invalid(),
            },
        }
        Ok(())
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
            Self::Font { .. } => "a font name, e.g. \"Charter\"".to_owned(),
            Self::Bool { .. } => one_of(&["true", "false"]),
            Self::OffOrInteger { range, .. } => format!(
                "`off` or a whole number from {} to {}",
                range.start(),
                range.end()
            ),
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

/// Checks a font family name: its name as installed, or `None` if it
/// isn't.
pub type FontCheck<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// Reads settings from the file's `text`, taking any font as installed.
/// Anything wrong is reported, and the affected settings keep their
/// defaults.
pub fn parse(text: &str) -> (Settings, Vec<Problem>) {
    parse_checking_fonts(text, &|name| Some(name.to_owned()))
}

/// Reads settings like [`parse`], reporting fonts that `installed` says
/// aren't installed.
pub fn parse_checking_fonts(text: &str, installed: &FontCheck) -> (Settings, Vec<Problem>) {
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
        let read = match item.as_value() {
            Some(value) => setting.kind.read(value, &mut settings, installed),
            None => Err(format!("must be {}", setting.kind.allowed())),
        };
        if let Err(problem) = read {
            problems.push(Problem {
                line,
                message: format!("`{key}` {problem}; using the default"),
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
    let Some(setting) = find(key) else {
        return Err(Problem {
            line: None,
            message: format!("unknown setting `{key}`"),
        });
    };
    let any_font = |name: &str| Some(name.to_owned());
    if let Err(problem) = setting
        .kind
        .read(&value, &mut Settings::default(), &any_font)
    {
        return Err(Problem {
            line: None,
            message: format!("`{key}` {problem}"),
        });
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

/// The file's `text` without `key`, so the setting takes its default,
/// keeping everything else. Fails if the file can't be parsed.
pub fn remove(text: &str, key: &str) -> Result<String, Problem> {
    let mut document: DocumentMut = text.parse().map_err(|err: toml_edit::TomlError| Problem {
        line: None,
        message: format!(
            "couldn't change `{key}` because the settings file has a problem: {}",
            err.message().trim_end()
        ),
    })?;
    if document.remove(key).is_none() {
        return Ok(text.to_owned());
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
                list_indent: ListIndent::Standard,
                body_font: None,
                code_font: None,
                editing_hint: true,
                autosave: None,
            }
        );
    }

    #[test]
    fn autosave_is_off_or_seconds() {
        assert_eq!(parse("autosave = 30\n").0.autosave, Some(30));
        assert_eq!(parse("autosave = \"off\"\n").0.autosave, None);
        for bad in ["0", "301", "\"on\"", "true", "2.5"] {
            let (settings, problems) = parse(&format!("autosave = {bad}\n"));
            assert_eq!(settings.autosave, None, "{bad}");
            assert_eq!(
                problems[0].message,
                "`autosave` must be `off` or a whole number from 1 to 300; using the default",
                "{bad}"
            );
        }
        assert!(set("", "autosave", 0).is_err());
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
    fn fonts() {
        let defaults = Settings::default();
        assert_eq!(
            (defaults.body_font.as_deref(), defaults.code_font()),
            (None, "Menlo")
        );
        let text = "body_font = \"charter\"\ncode_font = \"Nope Mono\"\n";
        let installed = |name: &str| {
            ["Charter", "Fira Code"]
                .into_iter()
                .find(|font| font.eq_ignore_ascii_case(name))
                .map(str::to_owned)
        };
        let (settings, problems) = parse_checking_fonts(text, &installed);
        assert_eq!(
            settings.body_font.as_deref(),
            Some("Charter"),
            "named as installed"
        );
        assert_eq!(settings.code_font, None);
        assert_eq!(
            problems[0].to_string(),
            "line 2: `code_font` names \"Nope Mono\", which isn't installed; using the default"
        );
        let (_, problems) = parse("body_font = 12\n");
        assert_eq!(
            problems[0].to_string(),
            "line 1: `body_font` must be a font name, e.g. \"Charter\"; using the default"
        );
    }

    #[test]
    fn remove_restores_the_default() {
        let text = "# Mine\ntheme = \"dark\" # night\nbody_font = \"Charter\"\n";
        assert_eq!(
            remove(text, "body_font").unwrap(),
            "# Mine\ntheme = \"dark\" # night\n"
        );
        assert_eq!(remove(text, "code_font").unwrap(), text);
        assert!(remove("x = = ", "theme").is_err());
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
                Kind::Font { get, .. } => {
                    let text = set("", setting.key, "Charter").unwrap();
                    assert_eq!(get(&parse(&text).0), Some("Charter"), "{}", setting.key);
                    let text = remove(&text, setting.key).unwrap();
                    assert_eq!(get(&parse(&text).0), None, "{}", setting.key);
                }
                Kind::Bool { get, .. } => {
                    for b in [true, false] {
                        let text = set("", setting.key, b).unwrap();
                        assert_eq!(get(&parse(&text).0), b, "{}", setting.key);
                    }
                }
                Kind::OffOrInteger { range, on, get, .. } => {
                    for n in [*range.start(), *on, *range.end()] {
                        let text = set("", setting.key, i64::from(n)).unwrap();
                        assert_eq!(get(&parse(&text).0), Some(n), "{}", setting.key);
                    }
                    let text = set("", setting.key, "off").unwrap();
                    assert_eq!(get(&parse(&text).0), None, "{}", setting.key);
                }
            }
        }
    }

    #[test]
    fn set_refuses_invalid_files() {
        assert!(set("theme = = ", "theme", "dark").is_err());
    }
}
