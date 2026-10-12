//! Drawing the revealed block: its Markdown source, styled, with the
//! selection and caret (C4).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Font, FontStyle, FontWeight, MouseButton, SharedString, StrikethroughStyle,
    StyledText, TextLayout, TextRun, UnderlineStyle, canvas, div, fill, prelude::*, px, relative,
    size,
};
use kami::reveal::{RevealFont, SpanStyle, StyledSource};

use crate::document_view::{BODY_TEXT_SIZE, CODE_FONT};
use crate::rendered_block::{ClickTarget, block_frame, heading_scale};
use crate::theme::Theme;

const CARET_WIDTH: f32 = 2.;

/// What to draw for the revealed block.
pub struct RevealedBlock<'a> {
    pub source: &'a str,
    pub styled: &'a StyledSource,
    /// The selection, in block-relative offsets, clamped to the block.
    pub selection: Range<usize>,
    /// The caret, in block-relative offsets, if it's in this block.
    pub caret: Option<usize>,
    pub first: bool,
}

/// Renders the revealed block as a list item. Its text layout is stored in
/// `layout_slot` for the view's layout-dependent movement.
pub fn render_revealed(
    block: RevealedBlock,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
    layout_slot: Rc<RefCell<Option<TextLayout>>>,
) -> AnyElement {
    let (heading, monospace) = match block.styled.font {
        RevealFont::Prose { heading } => (heading, false),
        RevealFont::Monospace => (None, true),
    };

    // Tabs are shown as single spaces so offsets stay identical to the
    // source.
    let text = SharedString::from(block.source.replace('\t', " "));
    let runs = text_runs(&block, base_font, theme, heading);
    let styled = StyledText::new(text).with_runs(runs);
    let layout = styled.layout().clone();
    *layout_slot.borrow_mut() = Some(layout.clone());

    let caret = block.caret;
    let caret_layout = layout.clone();
    let caret_overlay = canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let Some(caret) = caret else {
                return;
            };
            if let Some(position) = caret_layout.position_for_index(caret) {
                let height = caret_layout.line_height();
                window.paint_quad(fill(
                    Bounds::new(position, size(px(CARET_WIDTH), height)),
                    theme.caret,
                ));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();

    let click_text = click.clone();
    let text_element = div()
        .relative()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            click_text.report(index, event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .child(styled)
        .child(caret_overlay);

    let content = if monospace {
        div()
            .bg(theme.code_background)
            .rounded_md()
            .px_4()
            .py_3()
            .font_family(CODE_FONT)
            .text_size(px(13.))
            .line_height(relative(1.5))
            .child(text_element)
    } else if let Some(level) = heading {
        div()
            .text_size(px(BODY_TEXT_SIZE * heading_scale(level)))
            .line_height(relative(1.25))
            .child(text_element)
    } else {
        div().child(text_element)
    };

    block_frame(content.into_any_element(), heading, block.first, click)
}

/// Text runs for the styled source, with the selection as a background.
fn text_runs(
    block: &RevealedBlock,
    base_font: &Font,
    theme: Theme,
    heading: Option<u8>,
) -> Vec<TextRun> {
    let mut block_font = base_font.clone();
    if matches!(block.styled.font, RevealFont::Monospace) {
        block_font.family = CODE_FONT.into();
    }
    if heading.is_some() {
        block_font.weight = FontWeight::SEMIBOLD;
    }
    let base_color = if heading == Some(6) {
        theme.muted_text
    } else {
        theme.text
    };

    let mut runs = Vec::new();
    for span in &block.styled.spans {
        let mut font = block_font.clone();
        let mut color = base_color;
        let mut background = None;
        let mut underline = None;
        let mut strikethrough = None;
        match span.style {
            SpanStyle::Syntax => color = theme.muted_text,
            SpanStyle::Content(inline) => {
                if inline.strong {
                    font.weight = FontWeight::BOLD;
                }
                if inline.emphasis {
                    font.style = FontStyle::Italic;
                }
                if inline.code && block.styled.font != RevealFont::Monospace {
                    font.family = CODE_FONT.into();
                    background = Some(theme.code_background);
                }
                if inline.link {
                    color = theme.link;
                    underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(color),
                        wavy: false,
                    });
                }
                if inline.strikethrough {
                    strikethrough = Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(color),
                    });
                }
            }
        }

        // Split the span where the selection starts and ends.
        for piece in split_at_selection(span.range.clone(), &block.selection) {
            let selected = !block.selection.is_empty()
                && piece.start >= block.selection.start
                && piece.end <= block.selection.end;
            runs.push(TextRun {
                len: piece.len(),
                font: font.clone(),
                color,
                background_color: if selected {
                    Some(theme.selection)
                } else {
                    background
                },
                underline,
                strikethrough,
            });
        }
    }
    runs
}

/// Splits `range` at the selection's boundaries, dropping empty pieces.
fn split_at_selection(range: Range<usize>, selection: &Range<usize>) -> Vec<Range<usize>> {
    let mut cuts = vec![range.start];
    for cut in [selection.start, selection.end] {
        if range.start < cut && cut < range.end {
            cuts.push(cut);
        }
    }
    cuts.push(range.end);
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(
        clippy::single_range_in_vec_init,
        reason = "lists of one range are intended"
    )]
    fn splits_at_selection_edges() {
        assert_eq!(split_at_selection(0..10, &(3..6)), [0..3, 3..6, 6..10]);
        assert_eq!(split_at_selection(0..10, &(0..10)), vec![0..10]);
        assert_eq!(split_at_selection(0..10, &(8..20)), [0..8, 8..10]);
        assert_eq!(split_at_selection(5..10, &(0..2)), vec![5..10]);
        assert_eq!(split_at_selection(0..10, &(4..4)), [0..4, 4..10]);
    }
}
