//! How dim the blocks that aren't being edited are drawn (#121), in a
//! [`Theme`]'s colors. The theme itself is kertas-ui's.

pub use kertas_ui::Theme;
use kertas_ui::theme::{contrast, over};

/// The dimmest the blocks that aren't being edited are drawn (#121), as
/// an opacity.
pub(crate) const FADED_OPACITY: f32 = 0.5;

/// The least contrast faded text keeps against the background (#121).
const FADED_CONTRAST: f32 = 3.;

/// How opaque the blocks that aren't being edited are drawn (#121): as
/// dim as [`FADED_OPACITY`], or less if this theme's text would then
/// have less than 3:1 contrast against its background.
pub(crate) fn faded_opacity(theme: &Theme) -> f32 {
    let readable = |opacity: f32| {
        contrast(
            over(theme.text, opacity, theme.background),
            theme.background,
        ) >= FADED_CONTRAST
    };
    // In steps of 1%, from the dimmest up.
    (0..=100)
        .map(|step| FADED_OPACITY + (1. - FADED_OPACITY) * step as f32 / 100.)
        .find(|&opacity| readable(opacity))
        .unwrap_or(1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kertas_core::themes;

    #[test]
    fn faded_text_is_readable_in_every_builtin_theme() {
        for theme in themes::builtin() {
            for (half, palette) in [("light", &theme.light), ("dark", &theme.dark)] {
                let colors = Theme::from(palette);
                let faded = over(colors.text, faded_opacity(&colors), colors.background);
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
            faded_opacity(&Theme::from(if light { &theme.light } else { &theme.dark }))
        };
        // Kertas's own theme reaches the dimmest.
        assert_eq!(opacity(true), FADED_OPACITY);
        assert_eq!(opacity(false), FADED_OPACITY);
        let solarized = themes::builtin()
            .iter()
            .find(|theme| theme.name == "Solarized")
            .unwrap();
        let light = Theme::from(&solarized.light);
        assert!(faded_opacity(&light) > FADED_OPACITY);
    }
}
