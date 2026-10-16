//! The colors windows are drawn in (A2), from a color theme.
//!
//! The `theme` setting picks a theme, and the `appearance` setting its
//! light or dark colors, or follows the macOS appearance. Themes come from
//! theme files; see [`kertas::themes`].

use gpui::{App, Hsla, Window, WindowAppearance, rgba};
use kertas::rendered::AlertKind;
use kertas::settings::AppearanceSetting;
use kertas::themes::{self, Appearance, Color, Palette};

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
    /// Text drawn on the caret color, e.g. a default button's label.
    pub accent_text: Hsla,
    /// Background and border that mark the revealed block.
    pub revealed_background: Hsla,
    pub revealed_border: Hsla,
    /// GitHub alerts' colors, in [`AlertKind`] order.
    alerts: [Hsla; 5],
}

impl Theme {
    /// The colors for `window`: the `theme` setting's, in light or dark
    /// mode as the `appearance` setting picks.
    pub fn current(window: &Window, cx: &App) -> Self {
        let settings = app_settings::get(cx);
        let appearance = match settings.appearance {
            AppearanceSetting::Light => Appearance::Light,
            AppearanceSetting::Dark => Appearance::Dark,
            AppearanceSetting::System => appearance(window.appearance()),
        };
        // The setting only names a theme there is.
        let theme = themes::builtin()
            .iter()
            .find(|theme| theme.name == settings.theme)
            .unwrap_or_else(|| themes::kertas());
        Self::from(theme.palette(appearance))
    }

    /// Kertas's light colors, which printing uses.
    pub fn light() -> Self {
        Self::from(&themes::kertas().light)
    }

    /// Kertas's dark colors, which the smoke tests check for.
    #[cfg(debug_assertions)]
    pub fn dark() -> Self {
        Self::from(&themes::kertas().dark)
    }

    /// The color of an alert's bar, icon and title, as GitHub draws it.
    pub fn alert(&self, kind: AlertKind) -> Hsla {
        self.alerts[kind as usize]
    }
}

/// Whether a window's macOS appearance is light or dark.
fn appearance(appearance: WindowAppearance) -> Appearance {
    match appearance {
        WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
    }
}

impl From<&Palette> for Theme {
    fn from(palette: &Palette) -> Self {
        let color = |Color(rgba_value): Color| rgba(rgba_value).into();
        Self {
            background: color(palette.background),
            text: color(palette.text),
            muted_text: color(palette.muted_text),
            link: color(palette.link),
            code_background: color(palette.code_background),
            rule: color(palette.rule),
            table_header_background: color(palette.table_header_background),
            selection: color(palette.selection),
            caret: color(palette.caret),
            accent_text: color(palette.accent_text),
            revealed_background: color(palette.revealed_background),
            revealed_border: color(palette.revealed_border),
            alerts: [
                color(palette.alert_note),
                color(palette.alert_tip),
                color(palette.alert_important),
                color(palette.alert_warning),
                color(palette.alert_caution),
            ],
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
    fn text_is_readable_in_every_builtin_theme() {
        let halves = themes::builtin().iter().flat_map(|theme| {
            [("light", &theme.light), ("dark", &theme.dark)]
                .map(|(half, palette)| (&theme.name, half, palette))
        });
        for (theme_name, half, palette) in halves {
            let theme = Theme::from(palette);
            // WCAG AA: 4.5:1 for body text.
            for (name, color, background) in [
                ("text", theme.text, theme.background),
                ("muted text", theme.muted_text, theme.background),
                ("link", theme.link, theme.background),
                ("code", theme.text, theme.code_background),
                ("table header", theme.text, theme.table_header_background),
                ("accent text", theme.accent_text, theme.caret),
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
                assert!(
                    ratio >= 4.5,
                    "{name} contrast {ratio:.2} in {theme_name} ({half})"
                );
            }
        }
    }

    #[test]
    fn window_appearance_is_light_or_dark() {
        assert_eq!(appearance(WindowAppearance::Light), Appearance::Light);
        assert_eq!(
            appearance(WindowAppearance::VibrantLight),
            Appearance::Light
        );
        assert_eq!(appearance(WindowAppearance::Dark), Appearance::Dark);
        assert_eq!(appearance(WindowAppearance::VibrantDark), Appearance::Dark);
    }
}
