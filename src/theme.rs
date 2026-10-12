//! Built-in light and dark color themes (A2).
//!
//! The theme follows the macOS appearance. Colors are hardcoded for now;
//! the config file (§8) will add a setting to pin one.

use gpui::{Hsla, WindowAppearance, rgb, rgba};

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
}

impl Theme {
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
        }
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
