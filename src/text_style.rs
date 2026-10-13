//! How inline text styles look on screen: the one mapping from an
//! [`InlineStyle`] to a font, colour and decorations, used by rendered and
//! revealed text alike.

use gpui::{
    Font, FontStyle, FontWeight, Hsla, StrikethroughStyle, Styled, TextRun, UnderlineStyle, px,
    relative, rems,
};
use kami::inline::InlineStyle;
pub use kami::rendered::CODE_FONT;
use kami::rendered::heading_scale;

use crate::theme::Theme;

/// What document text is drawn with: the body font at the current size,
/// the theme, and the text settings.
#[derive(Clone)]
pub struct Look {
    pub font: Font,
    pub theme: Theme,
    /// How many spaces a tab is shown as.
    pub tab_width: u32,
}

/// Code's text size relative to body text (13 px at 16 px).
pub const CODE_TEXT_SIZE: f32 = 0.8125;

/// A run of `len` bytes of text in `style`, on top of `font` and `color`.
pub fn inline_run(
    style: InlineStyle,
    len: usize,
    font: &Font,
    color: Hsla,
    theme: Theme,
) -> TextRun {
    let mut font = font.clone();
    if style.code {
        font.family = CODE_FONT.into();
    }
    if style.strong {
        font.weight = FontWeight::BOLD;
    }
    if style.emphasis {
        font.style = FontStyle::Italic;
    }
    let color = if style.link { theme.link } else { color };
    TextRun {
        len,
        font,
        color,
        background_color: style.code.then_some(theme.code_background),
        underline: style.link.then(|| underline(color)),
        strikethrough: style.strikethrough.then(|| StrikethroughStyle {
            thickness: px(1.),
            color: Some(color),
        }),
    }
}

/// A thin underline, for links and text being composed.
pub fn underline(color: Hsla) -> UnderlineStyle {
    UnderlineStyle {
        thickness: px(1.),
        color: Some(color),
        wavy: false,
    }
}

/// `element` set in the code font and size: code blocks, and source shown
/// as-is.
pub fn code_text<E: Styled>(element: E) -> E {
    element
        .font_family(CODE_FONT)
        .text_size(rems(CODE_TEXT_SIZE))
        .line_height(relative(1.5))
}

/// `element` sized for a heading `level`.
pub fn heading_text<E: Styled>(element: E, level: u8) -> E {
    element
        .text_size(rems(heading_scale(level) as f32))
        .line_height(relative(1.25))
}
