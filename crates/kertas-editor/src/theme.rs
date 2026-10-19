//! The colors windows are drawn in (A2), from a color theme.
//!
//! A theme has light and dark colors. Which are shown is chosen, or follows
//! the macOS appearance. Themes come from theme files; see
//! [`kertas_core::themes`].

use gpui::{Hsla, Rgba, Window, WindowAppearance, rgba};
use kertas_core::rendered::AlertKind;
use kertas_core::settings::AppearanceSetting;
use kertas_core::themes::{self, Appearance, Color, Palette};

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
    /// Background of the text find matches, and of the match it's at
    /// (#122): the warning color, faintly and more strongly.
    pub find_match: Hsla,
    pub find_current: Hsla,
    /// GitHub alerts' colors, in [`AlertKind`] order.
    alerts: [Hsla; 5],
}

impl Theme {
    /// `theme`'s colors for `window`, in light or dark mode as
    /// `appearance` picks.
    pub fn for_window(
        theme: &themes::Theme,
        appearance_setting: AppearanceSetting,
        window: &Window,
    ) -> Self {
        let appearance = match appearance_setting {
            AppearanceSetting::Light => Appearance::Light,
            AppearanceSetting::Dark => Appearance::Dark,
            AppearanceSetting::System => appearance(window.appearance()),
        };
        Self::from(theme.palette(appearance))
    }

    /// The built-in Kertas theme's light colors.
    pub fn light() -> Self {
        Self::from(&themes::kertas().light)
    }

    /// The built-in Kertas theme's dark colors, for the smoke tests.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
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
            find_match: Hsla {
                a: 0.25,
                ..color(palette.alert_warning)
            },
            find_current: Hsla {
                a: 0.55,
                ..color(palette.alert_warning)
            },
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

/// The dimmest the blocks that aren't being edited are drawn (#121), as
/// an opacity.
pub(crate) const FADED_OPACITY: f32 = 0.5;

/// The least contrast faded text keeps against the background (#121).
const FADED_CONTRAST: f32 = 3.;

impl Theme {
    /// How opaque the blocks that aren't being edited are drawn (#121): as
    /// dim as [`FADED_OPACITY`], or less if this theme's text would then
    /// have less than 3:1 contrast against its background.
    pub(crate) fn faded_opacity(&self) -> f32 {
        let readable = |opacity: f32| {
            contrast(over(self.text, opacity, self.background), self.background) >= FADED_CONTRAST
        };
        // In steps of 1%, from the dimmest up.
        (0..=100)
            .map(|step| FADED_OPACITY + (1. - FADED_OPACITY) * step as f32 / 100.)
            .find(|&opacity| readable(opacity))
            .unwrap_or(1.)
    }
}

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
pub(crate) fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color` drawn at `opacity` over `background`.
pub(crate) fn over(color: Hsla, opacity: f32, background: Hsla) -> Hsla {
    let (color, background) = (Rgba::from(color), Rgba::from(background));
    let mix = |c: f32, b: f32| c * opacity + b * (1. - opacity);
    Hsla::from(Rgba {
        r: mix(color.r, background.r),
        g: mix(color.g, background.g),
        b: mix(color.b, background.b),
        a: 1.,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn faded_text_is_readable_in_every_builtin_theme() {
        for theme in themes::builtin() {
            for (half, palette) in [("light", &theme.light), ("dark", &theme.dark)] {
                let colors = Theme::from(palette);
                let faded = over(colors.text, colors.faded_opacity(), colors.background);
                let ratio = contrast(faded, colors.background);
                // #121: at least 3:1 for faded body text.
                assert!(
                    ratio >= 3.,
                    "faded text contrast {ratio:.2} in {} ({half})",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn faded_text_is_as_dim_as_it_can_be() {
        let opacity = |light: bool| {
            let theme = &themes::builtin()[0];
            Theme::from(if light { &theme.light } else { &theme.dark }).faded_opacity()
        };
        // Kertas's own theme reaches the dimmest.
        assert_eq!(opacity(true), FADED_OPACITY);
        assert_eq!(opacity(false), FADED_OPACITY);
        let solarized = themes::builtin()
            .iter()
            .find(|theme| theme.name == "Solarized")
            .unwrap();
        let light = Theme::from(&solarized.light);
        assert!(light.faded_opacity() > FADED_OPACITY);
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
