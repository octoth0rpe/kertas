//! Drawing the revealed block: its Markdown source, styled, with the
//! selection and caret (C4).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, ElementInputHandler, Entity, FocusHandle, Font, FontStyle, FontWeight,
    MouseButton, SharedString, StrikethroughStyle, StyledText, TextLayout, TextRun, UnderlineStyle,
    canvas, div, fill, prelude::*, px, relative, size,
};
use kami::model::Gap;
use kami::reveal::{RevealFont, SpanStyle, StyledSource};

use crate::document_view::{BODY_TEXT_SIZE, CODE_FONT, DocumentView};
use kami::rendered::RenderedBlock;

use crate::rendered_block::{
    ClickTarget, block_frame, heading_scale, rendered_content, rendered_heading,
};
use crate::theme::Theme;

const CARET_WIDTH: f32 = 2.;

/// How far the revealed block's highlight extends outside its text.
const HIGHLIGHT_INSET: f32 = 8.;

/// Shown on the revealed block's border.
const EDITING_HINT: &str = "esc to finish · ⌥↓ next block";
const HINT_HEIGHT: f32 = 14.;

/// Space between a block and the caret's line in the gap next to it.
const GAP_SPACE: f32 = 16.;

/// What to draw for the revealed block.
pub struct RevealedBlock<'a> {
    pub source: &'a str,
    pub styled: &'a StyledSource,
    /// The selection, in block-relative offsets, clamped to the block.
    pub selection: Range<usize>,
    /// The caret, in block-relative offsets, if it's in this block and
    /// currently drawn (it blinks).
    pub caret: Option<usize>,
    /// Text being composed with an input method, in block-relative offsets.
    pub marked: Option<Range<usize>>,
    pub first: bool,
}

/// Where typed text and IME input go while the block is revealed, and
/// where its text layout is stored once painted, for the view's
/// layout-dependent movement.
pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub view: Entity<DocumentView>,
    pub layout_slot: Rc<RefCell<Option<TextLayout>>>,
}

/// Renders the revealed block as a list item.
pub fn render_revealed(
    block: RevealedBlock,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
    input: TextInput,
) -> AnyElement {
    let heading = revealed_heading(block.styled);
    let first = block.first;
    let content = revealed_content(block, base_font, theme, click.clone(), input);
    block_frame(content, heading, first, click)
}

/// Renders a block in rendered mode with the caret on an empty line in the
/// gap before or after it, where typing starts a new block.
pub fn render_gap(
    block: &RenderedBlock,
    gap: Gap,
    caret: RevealedBlock,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
    input: TextInput,
) -> AnyElement {
    let first = caret.first;
    let heading = rendered_heading(block);
    let rendered = rendered_content(block, base_font, theme, click.clone());
    let line = revealed_content(caret, base_font, theme, click.clone(), input);
    let content = match gap {
        Gap::Above => div()
            .child(div().mb(px(GAP_SPACE)).child(line))
            .child(rendered),
        Gap::Below => div()
            .child(rendered)
            .child(div().mt(px(GAP_SPACE)).child(line)),
    };
    block_frame(content.into_any_element(), heading, first, click)
}

fn revealed_heading(styled: &StyledSource) -> Option<u8> {
    match styled.font {
        RevealFont::Prose { heading } => heading,
        RevealFont::Monospace => None,
    }
}

/// The revealed block's highlighted, editable source, without the list
/// item's spacing.
fn revealed_content(
    block: RevealedBlock,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
    input: TextInput,
) -> AnyElement {
    let heading = revealed_heading(block.styled);
    let monospace = block.styled.font == RevealFont::Monospace;

    // Tabs are shown as single spaces so offsets stay identical to the
    // source.
    let text = SharedString::from(block.source.replace('\t', " "));
    let runs = text_runs(&block, base_font, theme, heading);
    let styled = StyledText::new(text).with_runs(runs);
    let layout = styled.layout().clone();

    let caret = block.caret;
    let caret_layout = layout.clone();
    let caret_overlay = canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            // Publish the layout only now that it's been painted: its
            // position queries panic before prepaint, and the list can
            // build an item just to measure it without painting it.
            *input.layout_slot.borrow_mut() = Some(caret_layout.clone());
            window.handle_input(
                &input.focus_handle,
                ElementInputHandler::new(bounds, input.view),
                cx,
            );
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

    // Highlight the revealed block. The highlight extends outside the text
    // column by its padding plus border, so the text doesn't shift. Clicks
    // on its padding count as clicks in the column.
    let hint = div()
        .absolute()
        .right(px(12.))
        .bottom(px(-HINT_HEIGHT / 2.))
        .h(px(HINT_HEIGHT))
        .px(px(6.))
        .bg(theme.background)
        .text_size(px(11.))
        .line_height(px(HINT_HEIGHT))
        .text_color(theme.muted_text)
        .child(EDITING_HINT);
    div()
        .relative()
        .mx(px(-HIGHLIGHT_INSET))
        .my(px(-HIGHLIGHT_INSET))
        .p(px(HIGHLIGHT_INSET - 1.))
        .border_1()
        .border_color(theme.revealed_border)
        .rounded_md()
        .bg(theme.revealed_background)
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            click.report(0, event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .child(content)
        .child(hint)
        .into_any_element()
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

        // Split the span where the selection and marked text start and end.
        let marked = block.marked.clone().unwrap_or(0..0);
        let cuts = [
            block.selection.start,
            block.selection.end,
            marked.start,
            marked.end,
        ];
        for piece in split_at(span.range.clone(), &cuts) {
            let within = |range: &Range<usize>| {
                !range.is_empty() && piece.start >= range.start && piece.end <= range.end
            };
            let underline = if within(&marked) {
                Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(color),
                    wavy: false,
                })
            } else {
                underline
            };
            runs.push(TextRun {
                len: piece.len(),
                font: font.clone(),
                color,
                background_color: if within(&block.selection) {
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

/// Splits `range` at any of `cuts` that fall inside it, dropping empty
/// pieces.
fn split_at(range: Range<usize>, cuts: &[usize]) -> Vec<Range<usize>> {
    let mut points = vec![range.start];
    points.extend(
        cuts.iter()
            .copied()
            .filter(|&cut| range.start < cut && cut < range.end),
    );
    points.push(range.end);
    points.sort_unstable();
    points.dedup();
    points.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_cuts_inside_range() {
        assert_eq!(split_at(0..10, &[3, 6]), [0..3, 3..6, 6..10]);
        assert_eq!(split_at(0..10, &[0, 10]), vec![0..10]);
        assert_eq!(split_at(0..10, &[8, 20]), [0..8, 8..10]);
        assert_eq!(split_at(5..10, &[0, 2]), vec![5..10]);
        assert_eq!(split_at(0..10, &[4, 4, 6, 2]), [0..2, 2..4, 4..6, 6..10]);
    }
}
