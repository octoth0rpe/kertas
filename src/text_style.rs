//! How inline text styles look on screen: the one mapping from an
//! [`InlineStyle`] to a font, colour and decorations, used by rendered and
//! revealed text alike.

use gpui::{
    Font, FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, Styled, TextRun,
    UnderlineStyle, px, relative, rems,
};
use kertas::inline::InlineStyle;
use kertas::rendered::heading_scale;
use kertas::settings::{ListIndent, Settings, TextWidth};

use crate::theme::Theme;

/// The settings that change how document text is laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSettings {
    pub text_width: TextWidth,
    pub tab_width: u32,
    pub list_indent: ListIndent,
    /// The font family for text, or `None` for the system font.
    pub body_font: Option<SharedString>,
    /// The font family for code.
    pub code_font: SharedString,
}

impl From<&Settings> for TextSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            text_width: settings.text_width,
            tab_width: settings.tab_width,
            list_indent: settings.list_indent,
            body_font: settings.body_font.clone().map(SharedString::from),
            code_font: settings.code_font().to_owned().into(),
        }
    }
}

/// What document text is drawn with: the body and code fonts, the theme,
/// the tab width and the list indent.
#[derive(Clone)]
pub struct Look {
    /// The body font, at the current size.
    pub font: Font,
    /// The code font's family.
    pub code_font: SharedString,
    pub theme: Theme,
    /// How many spaces a tab is shown as.
    pub tab_width: u32,
    pub list_indent: ListIndent,
}

impl Look {
    /// The look for `settings`, on top of the window's `font`.
    pub fn new(mut font: Font, theme: Theme, settings: &TextSettings) -> Self {
        if let Some(body_font) = &settings.body_font {
            font.family = body_font.clone();
        }
        Self {
            font,
            code_font: settings.code_font.clone(),
            theme,
            tab_width: settings.tab_width,
            list_indent: settings.list_indent,
        }
    }
}

/// Code's text size relative to body text (13 px at 16 px).
pub const CODE_TEXT_SIZE: f32 = 0.8125;

/// A run of `len` bytes of text in `style`, on top of `font` and `color`,
/// with code in `code_font`.
pub fn inline_run(
    style: InlineStyle,
    len: usize,
    font: &Font,
    color: Hsla,
    theme: Theme,
    code_font: &SharedString,
) -> TextRun {
    let mut font = font.clone();
    if style.code {
        font.family = code_font.clone();
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

/// `element` set in `code_font` at the code size: code blocks, and source
/// shown as-is.
pub fn code_text<E: Styled>(element: E, code_font: &SharedString) -> E {
    element
        .font_family(code_font.clone())
        .text_size(rems(CODE_TEXT_SIZE))
        .line_height(relative(1.5))
}

/// `element` sized for a heading `level`.
pub fn heading_text<E: Styled>(element: E, level: u8) -> E {
    element
        .text_size(rems(heading_scale(level) as f32))
        .line_height(relative(1.25))
}
