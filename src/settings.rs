//! kami's settings (REQUIREMENTS §8): what they are, and reading and
//! writing them in the TOML settings file.
//!
//! The file is optional and every setting has a default (G2). Problems in
//! it never stop kami: a bad value or an unknown setting falls back to the
//! default and is reported with its line (G4). Changing a setting edits
//! the file in place, keeping comments and formatting.

use std::fmt;

use toml_edit::{DocumentMut, Item, Value};

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

/// All settings, with defaults for any the file doesn't set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeSetting,
    pub layout: Layout,
    pub text_width: TextWidth,
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

/// A setting's name in the file and its allowed values.
struct Setting {
    key: &'static str,
    values: &'static [&'static str],
    apply: fn(&mut Settings, &str),
}

const SETTINGS: &[Setting] = &[
    Setting {
        key: "theme",
        values: &["light", "dark", "system"],
        apply: |settings, value| {
            settings.theme = match value {
                "light" => ThemeSetting::Light,
                "dark" => ThemeSetting::Dark,
                _ => ThemeSetting::System,
            }
        },
    },
    Setting {
        key: "layout",
        values: &["tabs", "windows"],
        apply: |settings, value| {
            settings.layout = match value {
                "windows" => Layout::Windows,
                _ => Layout::Tabs,
            }
        },
    },
    Setting {
        key: "text_width",
        values: &["standard", "wide", "full"],
        apply: |settings, value| {
            settings.text_width = match value {
                "wide" => TextWidth::Wide,
                "full" => TextWidth::Full,
                _ => TextWidth::Standard,
            }
        },
    },
];

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
        let Some(setting) = SETTINGS.iter().find(|setting| setting.key == key) else {
            problems.push(Problem {
                line,
                message: format!("unknown setting `{key}`"),
            });
            continue;
        };
        match item.as_str() {
            Some(value) if setting.values.contains(&value) => (setting.apply)(&mut settings, value),
            _ => problems.push(Problem {
                line,
                message: format!(
                    "`{key}` must be {}; using the default",
                    one_of(setting.values)
                ),
            }),
        }
    }
    (settings, problems)
}

/// The file's `text` with `key` set to `value`, keeping everything else,
/// including comments. Fails if the file can't be parsed, so it's never
/// overwritten.
pub fn set(text: &str, key: &str, value: &str) -> Result<String, Problem> {
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
            *existing = Value::from(value);
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
            line[key] = toml_edit::value(value);
            text.push_str(&line.to_string());
            return Ok(text);
        }
        _ => document[key] = toml_edit::value(value),
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
    fn set_refuses_invalid_files() {
        assert!(set("theme = = ", "theme", "dark").is_err());
    }

    #[test]
    fn values_round_trip() {
        for theme in [
            ThemeSetting::Light,
            ThemeSetting::Dark,
            ThemeSetting::System,
        ] {
            let text = set("", "theme", theme.as_str()).unwrap();
            assert_eq!(parse(&text).0.theme, theme);
        }
        for layout in [Layout::Tabs, Layout::Windows] {
            let text = set("", "layout", layout.as_str()).unwrap();
            assert_eq!(parse(&text).0.layout, layout);
        }
        for width in [TextWidth::Standard, TextWidth::Wide, TextWidth::Full] {
            let text = set("", "text_width", width.as_str()).unwrap();
            assert_eq!(parse(&text).0.text_width, width);
        }
    }
}
