//! Built-in light and dark color themes (A2).
//!
//! The `theme` setting picks one, or follows the macOS appearance.

use gpui::{App, Hsla, Window, WindowAppearance, rgb, rgba};
use kertas::rendered::AlertKind;
use kertas::settings::ThemeSetting;

use crate::app_settings;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub background: Hsla,
    pub text: Hsla,
    /// Secondary text: blockquotes, h6, source shown as-is, hints.
    pub muted_text: Hsla,
    pub link: Hsla,
    pub code_background: Hsla,
    /// Thematic breaks, blockquote bars and table borders.
    pub rule: Hsla,
    pub table_header_background: Hsla,
    /// Background of selected text.
    pub selection: Hsla,
    pub caret: Hsla,
    /// Background and border that mark the revealed block.
    pub revealed_background: Hsla,
    pub revealed_border: Hsla,
    /// GitHub alerts' colors, in [`AlertKind`] order.
    alerts: [Hsla; 5],
}

impl Theme {
    /// The theme for `window`, from the `theme` setting.
    pub fn current(window: &Window, cx: &App) -> Self {
        match app_settings::get(cx).theme {
            ThemeSetting::Light => Self::light(),
            ThemeSetting::Dark => Self::dark(),
            ThemeSetting::System => Self::for_appearance(window.appearance()),
        }
    }

    pub fn light() -> Self {
        Self {
            background: rgb(0xffffff).into(),
            text: rgb(0x1f2328).into(),
            muted_text: rgb(0x59636e).into(),
            link: rgb(0x0969da).into(),
            code_background: rgb(0xeff1f3).into(),
            rule: rgb(0xd1d9e0).into(),
            table_header_background: rgb(0xf6f8fa).into(),
            selection: rgba(0x0969da40).into(),
            caret: rgb(0x0969da).into(),
            revealed_background: rgba(0x0969da0a).into(),
            revealed_border: rgba(0x0969da40).into(),
            alerts: [
                rgb(0x0969da).into(),
                rgb(0x1a7f37).into(),
                rgb(0x8250df).into(),
                rgb(0x9a6700).into(),
                rgb(0xd1242f).into(),
            ],
        }
    }

    pub fn dark() -> Self {
        Self {
            background: rgb(0x16181d).into(),
            text: rgb(0xe6e8eb).into(),
            muted_text: rgb(0x9aa3ad).into(),
            link: rgb(0x58a6ff).into(),
            code_background: rgb(0x262a31).into(),
            rule: rgb(0x3a4049).into(),
            table_header_background: rgb(0x1e2127).into(),
            selection: rgba(0x58a6ff4d).into(),
            caret: rgb(0x58a6ff).into(),
            revealed_background: rgba(0x58a6ff0f).into(),
            revealed_border: rgba(0x58a6ff47).into(),
            alerts: [
                rgb(0x4493f8).into(),
                rgb(0x3fb950).into(),
                rgb(0xab7df8).into(),
                rgb(0xd29922).into(),
                rgb(0xf85149).into(),
            ],
        }
    }

    /// The color of an alert's bar, icon and title, as GitHub draws it.
    pub fn alert(&self, kind: AlertKind) -> Hsla {
        self.alerts[kind as usize]
    }

    /// The theme matching a window's macOS appearance.
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::Rgba;

    use super::*;

    /// WCAG relative luminance.
    fn luminance(color: Hsla) -> f32 {
        let rgba = Rgba::from(color);
        let channel = |c: f32| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
    }

    /// WCAG contrast ratio between two colors, from 1 to 21.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_is_readable_in_both_themes() {
        for theme in [Theme::light(), Theme::dark()] {
            // WCAG AA: 4.5:1 for body text.
            for (name, color, background) in [
                ("text", theme.text, theme.background),
                ("muted text", theme.muted_text, theme.background),
                ("link", theme.link, theme.background),
                ("code", theme.text, theme.code_background),
                ("table header", theme.text, theme.table_header_background),
                ("note", theme.alert(AlertKind::Note), theme.background),
                ("tip", theme.alert(AlertKind::Tip), theme.background),
                (
                    "important",
                    theme.alert(AlertKind::Important),
                    theme.background,
                ),
                ("warning", theme.alert(AlertKind::Warning), theme.background),
                ("caution", theme.alert(AlertKind::Caution), theme.background),
            ] {
                let ratio = contrast(color, background);
                assert!(ratio >= 4.5, "{name} contrast {ratio:.2} in {theme:?}");
            }
        }
    }

    #[test]
    fn appearance_picks_theme() {
        assert_eq!(
            Theme::for_appearance(WindowAppearance::Light),
            Theme::light()
        );
        assert_eq!(
            Theme::for_appearance(WindowAppearance::VibrantLight),
            Theme::light()
        );
        assert_eq!(Theme::for_appearance(WindowAppearance::Dark), Theme::dark());
        assert_eq!(
            Theme::for_appearance(WindowAppearance::VibrantDark),
            Theme::dark()
        );
    }
}
