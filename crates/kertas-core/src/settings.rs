//! Kertas's settings (REQUIREMENTS §8): what they are, and reading and
//! writing them in the TOML settings file.
//!
//! The file is optional and every setting has a default (G2). Problems in
//! it never stop Kertas: a bad value or an unknown setting falls back to the
//! default and is reported with its line (G4). Changing a setting edits
//! the file in place, keeping comments and formatting.

use std::fmt;
use std::ops::RangeInclusive;

use toml_edit::{DocumentMut, Item, Value};

use crate::dictionaries::{self, DEFAULT_LANGUAGE};
use crate::rendered::CODE_FONT;
use crate::tabs::{DEFAULT_TAB_WIDTH, TAB_WIDTHS};
use crate::themes;

/// Light or dark mode (A2), which picks a theme's light or dark colors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppearanceSetting {
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

/// The color theme used unless one is set.
pub const DEFAULT_THEME: &str = "Kertas";

/// All settings, with defaults for any the file doesn't set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub appearance: AppearanceSetting,
    /// The color theme's name.
    pub theme: String,
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
    /// Whether documents show a scrollbar (#88).
    pub scrollbar: bool,
    /// Whether the blocks that aren't being edited are drawn faded (#121).
    pub fade_other_blocks: bool,
    /// Whether the caret's line is kept at the middle of the window as
    /// you type (#128).
    pub typewriter_scrolling: bool,
    /// Save documents once typing has paused this many seconds, or
    /// `None` not to save them automatically (F6).
    pub autosave: Option<u32>,
    /// The language spelling is checked in, as a language tag naming a
    /// dictionary (#140).
    pub spelling_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: AppearanceSetting::default(),
            theme: DEFAULT_THEME.to_owned(),
            layout: Layout::default(),
            text_width: TextWidth::default(),
            font_size: DEFAULT_FONT_SIZE,
            tab_width: DEFAULT_TAB_WIDTH,
            list_indent: ListIndent::default(),
            body_font: None,
            code_font: None,
            editing_hint: true,
            scrollbar: true,
            fade_other_blocks: false,
            typewriter_scrolling: false,
            autosave: None,
            spelling_language: DEFAULT_LANGUAGE.to_owned(),
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
    /// A color theme's name.
    Theme {
        get: fn(&Settings) -> &str,
        apply: fn(&mut Settings, String),
    },
    /// A language tag, naming a spelling dictionary. Whether there's a
    /// dictionary for it isn't checked here, since a missing one keeps
    /// the language, unlike a missing theme.
    Language {
        get: fn(&Settings) -> &str,
        apply: fn(&mut Settings, String),
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

/// The `appearance` setting's values, and their labels.
const APPEARANCES: &[(&str, &str)] = &[
    ("light", "Light"),
    ("dark", "Dark"),
    ("system", "Match System"),
];

pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "appearance",
        label: "Appearance",
        note: None,
        kind: Kind::Choice {
            choices: APPEARANCES,
            get: |settings| settings.appearance.as_str(),
            apply: |settings, value| settings.appearance = AppearanceSetting::parse(value),
        },
    },
    Setting {
        key: "theme",
        label: "Theme",
        note: None,
        kind: Kind::Theme {
            get: |settings| &settings.theme,
            apply: |settings, name| settings.theme = name,
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
        key: "scrollbar",
        label: "Scrollbar",
        note: Some("Show a scrollbar beside the document"),
        kind: Kind::Bool {
            get: |settings| settings.scrollbar,
            apply: |settings, show| settings.scrollbar = show,
        },
    },
    Setting {
        key: "fade_other_blocks",
        label: "Fade other blocks",
        note: Some("Fade every block but the one being edited"),
        kind: Kind::Bool {
            get: |settings| settings.fade_other_blocks,
            apply: |settings, fade| settings.fade_other_blocks = fade,
        },
    },
    Setting {
        key: "typewriter_scrolling",
        label: "Typewriter scrolling",
        note: Some("Keep the line being typed in the middle of the window"),
        kind: Kind::Bool {
            get: |settings| settings.typewriter_scrolling,
            apply: |settings, on| settings.typewriter_scrolling = on,
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
        key: "spelling_language",
        label: "Spelling",
        note: None,
        kind: Kind::Language {
            get: |settings| &settings.spelling_language,
            apply: |settings, tag| settings.spelling_language = tag,
        },
    },
    Setting {
        key: "layout",
        label: "Open documents in",
        note: Some("Takes effect the next time Kertas starts."),
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
    /// "`key` …". `available` says which fonts and themes there are.
    fn read(
        &self,
        value: &Value,
        settings: &mut Settings,
        available: &Available,
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
                Some(name) => match (available.fonts)(name) {
                    Some(name) => apply(settings, Some(name)),
                    None => return Err(format!("names \"{name}\", which isn't installed")),
                },
                None => return invalid(),
            },
            Self::Theme { apply, .. } => match value.as_str() {
                Some(name) => match (available.themes)(name) {
                    Some(name) => apply(settings, name),
                    None => return Err(format!("names \"{name}\", which isn't a theme")),
                },
                None => return invalid(),
            },
            Self::Language { apply, .. } => match value.as_str() {
                Some(tag) if dictionaries::is_language_tag(tag) => apply(settings, tag.to_owned()),
                _ => return invalid(),
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
            Self::Theme { .. } => "a theme's name, e.g. \"Kertas\"".to_owned(),
            Self::Language { .. } => "a language tag, e.g. \"en-GB\"".to_owned(),
            Self::Bool { .. } => one_of(&["true", "false"]),
            Self::OffOrInteger { range, .. } => format!(
                "`off` or a whole number from {} to {}",
                range.start(),
                range.end()
            ),
        }
    }
}

impl AppearanceSetting {
    /// The appearance a value in the settings file names: `system` for
    /// anything but `light` or `dark`.
    fn parse(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

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

/// Checks a color theme's name: the theme's name as it's spelled, or
/// `None` if there's no such theme.
pub type ThemeCheck<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// The fonts and color themes settings can name.
pub struct Available<'a> {
    pub fonts: &'a FontCheck<'a>,
    pub themes: &'a ThemeCheck<'a>,
}

/// Reads settings from the file's `text`, taking any font as installed,
/// and knowing only the built-in themes. Anything wrong is reported, and
/// the affected settings keep their defaults.
pub fn parse(text: &str) -> (Settings, Vec<Problem>) {
    let available = Available {
        fonts: &|name| Some(name.to_owned()),
        themes: &|name| themes::find(themes::builtin(), name),
    };
    parse_checking(text, &available)
}

/// Reads settings like [`parse`], reporting fonts and themes that
/// `available` says there aren't.
pub fn parse_checking(text: &str, available: &Available) -> (Settings, Vec<Problem>) {
    parse_checking_from(text, &Settings::default(), available)
}

/// Reads settings like [`parse_checking`], with `defaults` for any the
/// file doesn't set, or sets wrongly. A host app gives its own defaults
/// this way.
pub fn parse_checking_from(
    text: &str,
    defaults: &Settings,
    available: &Available,
) -> (Settings, Vec<Problem>) {
    let mut settings = defaults.clone();
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
    let legacy = legacy_appearance(&document);
    for (key, item) in document.iter() {
        let line = document
            .get_key_value(key)
            .and_then(|(key, _)| key.span())
            .map(|span| line_of(span.start));
        // Before themes, `theme` was the appearance.
        if let Some(appearance) = legacy
            && key == "theme"
        {
            settings.appearance = AppearanceSetting::parse(appearance);
            continue;
        }
        let Some(setting) = find(key) else {
            problems.push(Problem {
                line,
                message: format!("unknown setting `{key}`"),
            });
            continue;
        };
        let read = match item.as_value() {
            Some(value) => setting.kind.read(value, &mut settings, available),
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
    let text = &migrate(text, key);
    let Some(setting) = find(key) else {
        return Err(Problem {
            line: None,
            message: format!("unknown setting `{key}`"),
        });
    };
    // The settings sheet offers only fonts and themes there are.
    let any = Available {
        fonts: &|name| Some(name.to_owned()),
        themes: &|name| Some(name.to_owned()),
    };
    if let Err(problem) = setting.kind.read(&value, &mut Settings::default(), &any) {
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
    let text = &migrate(text, key);
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

/// The appearance an old settings file gives as `theme`, from before
/// there were themes, if it has no `appearance`.
fn legacy_appearance(document: &toml_edit::Document<&str>) -> Option<&'static str> {
    if document.contains_key("appearance") {
        return None;
    }
    let value = document.get("theme")?.as_str()?;
    APPEARANCES
        .iter()
        .map(|&(appearance, _)| appearance)
        .find(|&appearance| appearance == value)
}

/// Before changing `key`, if it's `appearance` or `theme`, renames an
/// old file's `theme = "dark"` (say) to `appearance = "dark"`, keeping
/// its place and comments, so neither setting is lost.
fn migrate(text: &str, key: &str) -> String {
    let span = match toml_edit::Document::parse(text) {
        Ok(document)
            if ["appearance", "theme"].contains(&key) && legacy_appearance(&document).is_some() =>
        {
            document
                .get_key_value("theme")
                .and_then(|(key, _)| key.span())
        }
        _ => None,
    };
    match span {
        Some(span) => format!("{}appearance{}", &text[..span.start], &text[span.end..]),
        None => text.to_owned(),
    }
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
        assert_eq!(Settings::default().appearance, AppearanceSetting::System);
        assert_eq!(Settings::default().layout, Layout::Tabs);
    }

    #[test]
    fn reads_settings() {
        let (settings, problems) =
            parse("# Mine\nappearance = \"dark\"\nlayout = \"windows\"\ntext_width = \"wide\"\n");
        assert_eq!(problems, vec![]);
        assert_eq!(
            settings,
            Settings {
                appearance: AppearanceSetting::Dark,
                theme: DEFAULT_THEME.into(),
                layout: Layout::Windows,
                text_width: TextWidth::Wide,
                font_size: DEFAULT_FONT_SIZE,
                tab_width: DEFAULT_TAB_WIDTH,
                list_indent: ListIndent::Standard,
                body_font: None,
                code_font: None,
                editing_hint: true,
                scrollbar: true,
                fade_other_blocks: false,
                typewriter_scrolling: false,
                autosave: None,
                spelling_language: "en-US".into(),
            }
        );
    }

    #[test]
    fn scrollbar() {
        assert!(Settings::default().scrollbar);
        assert!(!parse("scrollbar = false\n").0.scrollbar);
        assert_eq!(set("", "scrollbar", false).unwrap(), "scrollbar = false\n");
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
        let (settings, problems) = parse("layout = \"windows\"\n\nappearance = \"blue\"\n");
        assert_eq!(settings.appearance, AppearanceSetting::System);
        assert_eq!(
            settings.layout,
            Layout::Windows,
            "good settings still apply"
        );
        assert_eq!(
            problems,
            vec![Problem {
                line: Some(3),
                message: "`appearance` must be `light`, `dark` or `system`; using the default"
                    .into(),
            }]
        );
        let (_, problems) = parse("appearance = 3\n");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn unknown_settings_are_reported() {
        let (settings, problems) = parse("appearance = \"light\"\nfont = \"x\"\n");
        assert_eq!(settings.appearance, AppearanceSetting::Light);
        assert_eq!(problems[0].to_string(), "line 2: unknown setting `font`");
    }

    #[test]
    fn invalid_toml_uses_defaults() {
        let (settings, problems) = parse("appearance = \"dark\"\nappearance = = \n");
        assert_eq!(settings, Settings::default());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn set_keeps_comments_and_formatting() {
        let text = "# My settings\nappearance = \"dark\"  # for night work\n\nlayout = \"tabs\"\n";
        assert_eq!(
            set(text, "appearance", "light").unwrap(),
            "# My settings\nappearance = \"light\"  # for night work\n\nlayout = \"tabs\"\n"
        );
    }

    #[test]
    fn set_adds_missing_settings() {
        assert_eq!(
            set("", "appearance", "dark").unwrap(),
            "appearance = \"dark\"\n"
        );
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
    fn fade_other_blocks() {
        assert!(!Settings::default().fade_other_blocks);
        assert!(parse("fade_other_blocks = true\n").0.fade_other_blocks);
        assert_eq!(
            set("", "fade_other_blocks", true).unwrap(),
            "fade_other_blocks = true\n"
        );
    }

    #[test]
    fn typewriter_scrolling() {
        assert!(!Settings::default().typewriter_scrolling);
        assert!(
            parse("typewriter_scrolling = true\n")
                .0
                .typewriter_scrolling
        );
        assert_eq!(
            set("", "typewriter_scrolling", true).unwrap(),
            "typewriter_scrolling = true\n"
        );
    }

    #[test]
    fn a_host_gives_its_own_defaults() {
        let available = Available {
            fonts: &|name| Some(name.to_owned()),
            themes: &|name| themes::find(themes::builtin(), name),
        };
        let defaults = Settings {
            fade_other_blocks: true,
            ..Settings::default()
        };
        let read = |text| parse_checking_from(text, &defaults, &available).0;
        assert!(read("").fade_other_blocks);
        assert!(!read("fade_other_blocks = false\n").fade_other_blocks);
        // A wrong value falls back to the host's default.
        assert!(read("fade_other_blocks = 3\n").fade_other_blocks);
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
        let available = Available {
            fonts: &installed,
            themes: &|_| None,
        };
        let (settings, problems) = parse_checking(text, &available);
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
    fn themes() {
        assert_eq!(Settings::default().theme, "Kertas");
        let available = Available {
            fonts: &|name| Some(name.to_owned()),
            themes: &|name| {
                ["Kertas", "Solarized"]
                    .into_iter()
                    .find(|theme| theme.eq_ignore_ascii_case(name))
                    .map(str::to_owned)
            },
        };
        let (settings, problems) = parse_checking("theme = \"solarized\"\n", &available);
        assert_eq!(problems, vec![]);
        assert_eq!(settings.theme, "Solarized", "named as spelled");

        let (settings, problems) = parse_checking("theme = \"Nope\"\n", &available);
        assert_eq!(settings.theme, DEFAULT_THEME);
        assert_eq!(
            problems[0].to_string(),
            "line 1: `theme` names \"Nope\", which isn't a theme; using the default"
        );
        let (_, problems) = parse("theme = true\n");
        assert_eq!(
            problems[0].to_string(),
            "line 1: `theme` must be a theme's name, e.g. \"Kertas\"; using the default"
        );
    }

    #[test]
    fn old_theme_setting_is_the_appearance() {
        // Before themes, `theme` was the appearance.
        let old = "# Mine\ntheme = \"dark\"  # at night\nlayout = \"windows\"\n";
        let (settings, problems) = parse(old);
        assert_eq!(problems, vec![]);
        assert_eq!(settings.appearance, AppearanceSetting::Dark);
        assert_eq!(settings.theme, DEFAULT_THEME);
        // Not when there's an `appearance`: then it names a theme.
        let (settings, problems) = parse("appearance = \"light\"\ntheme = \"dark\"\n");
        assert_eq!(settings.appearance, AppearanceSetting::Light);
        assert_eq!(problems.len(), 1);

        // Changing either renames it in place, so neither is lost.
        let renamed = "# Mine\nappearance = \"dark\"  # at night\nlayout = \"windows\"\n";
        assert_eq!(
            set(old, "theme", "Kertas").unwrap(),
            format!("{renamed}theme = \"Kertas\"\n")
        );
        assert_eq!(
            set(old, "appearance", "light").unwrap(),
            renamed.replace("dark", "light")
        );
        assert_eq!(remove(old, "theme").unwrap(), renamed);
        // Changing anything else leaves it.
        assert_eq!(
            set(old, "layout", "tabs").unwrap(),
            old.replace("windows", "tabs")
        );
    }

    #[test]
    fn remove_restores_the_default() {
        let text = "# Mine\nappearance = \"dark\" # night\nbody_font = \"Charter\"\n";
        assert_eq!(
            remove(text, "body_font").unwrap(),
            "# Mine\nappearance = \"dark\" # night\n"
        );
        assert_eq!(remove(text, "code_font").unwrap(), text);
        assert!(remove("x = = ", "appearance").is_err());
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
        let text = "appearance = \"dark\"\n\n[extra]\nx = 1\n";
        let (settings, problems) = parse(text);
        assert_eq!(settings.appearance, AppearanceSetting::Dark);
        assert_eq!(problems[0].to_string(), "line 3: unknown setting `extra`");
        // A new setting goes before the table, so it isn't inside it.
        let changed = set(text, "layout", "windows").unwrap();
        assert_eq!(
            changed,
            "appearance = \"dark\"\nlayout = \"windows\"\n\n[extra]\nx = 1\n"
        );
        assert_eq!(parse(&changed).0.layout, Layout::Windows);
        // A setting written as a table is replaced by a value.
        let changed = set("[appearance]\nx = 1\n", "appearance", "light").unwrap();
        assert_eq!(
            parse(&changed),
            (
                Settings {
                    appearance: AppearanceSetting::Light,
                    ..Settings::default()
                },
                vec![]
            )
        );
    }

    #[test]
    fn line_numbers_with_crlf_and_duplicates() {
        let (_, problems) = parse("appearance = \"dark\"\r\n\r\nlayout = 1\r\n");
        assert_eq!(problems[0].line, Some(3));
        let (settings, problems) = parse("appearance = \"dark\"\nappearance = \"light\"\n");
        assert_eq!(settings, Settings::default());
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn set_refuses_bad_values() {
        assert!(set("", "appearance", "blue").is_err());
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
                Kind::Theme { get, .. } => {
                    let text = set("", setting.key, "kertas").unwrap();
                    assert_eq!(get(&parse(&text).0), "Kertas", "{}", setting.key);
                    let text = remove(&text, setting.key).unwrap();
                    assert_eq!(get(&parse(&text).0), DEFAULT_THEME, "{}", setting.key);
                }
                Kind::Language { get, .. } => {
                    let text = set("", setting.key, "en-GB").unwrap();
                    assert_eq!(get(&parse(&text).0), "en-GB", "{}", setting.key);
                    assert!(set("", setting.key, "en US").is_err(), "{}", setting.key);
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
        assert!(set("appearance = = ", "appearance", "dark").is_err());
    }
}
