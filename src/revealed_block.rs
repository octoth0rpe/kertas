//! Drawing the revealed block: its Markdown source, styled, with the
//! selection and caret (C4).

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, ElementInputHandler, Entity, FocusHandle, FontWeight, MouseButton, Pixels,
    Point, SharedString, StyledText, TextLayout, TextRun, canvas, div, fill, point, prelude::*, px,
    rems, size,
};
use kertas_core::inline::InlineStyle;
use kertas_core::model::Gap;
use kertas_core::reveal::{RevealFont, SpanStyle, StyledSource};
use kertas_core::tabs::ExpandedTabs;

use crate::document_view::DocumentView;
use crate::text_style::{Look, code_text, heading_text, inline_run, underline};
use kertas_core::rendered::RenderedBlock;

use crate::rendered_block::{ClickTarget, block_frame, rendered_content, rendered_heading};

const CARET_WIDTH: f32 = 2.;

/// How far the revealed block's highlight extends outside its text.
const HIGHLIGHT_INSET: f32 = 8.;

/// Shown on the revealed block's border.
const EDITING_HINT: &str = "esc to finish · ⌥↑ previous block · ⌥↓ next block";
/// The hint's height and text size, in rems, so they scale with zoom.
const HINT_HEIGHT: f32 = 0.875;
const HINT_TEXT_SIZE: f32 = 0.6875;

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
    pub layout_slot: Rc<RefCell<Option<RevealedLayout>>>,
    /// Whether to show the editing hint (the `editing_hint` setting).
    pub hint: bool,
    /// Set if the view asked for the caret to be scrolled into view in
    /// this frame.
    pub scroll_to_caret: Option<ScrollToCaret>,
}

/// Where to scroll the caret's item to, besides showing the caret's line.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CaretPlace {
    /// Wherever it is: scroll only as far as the caret's line needs.
    Nearest,
    /// The item's top this far below the window's top: where it was
    /// before blocks were revealed (C7), or at the top.
    Top(Pixels),
    /// The item's bottom at the window's bottom, as when scrolling down
    /// to it.
    Bottom,
}

/// The caret to scroll into view in this frame, and where to put its
/// item. The view makes the item the list's first, so the list lays it out
/// whatever it knows of the heights of the items before it. Then the
/// caret's element asks the list to scroll to a window-sized space around
/// the item, which the list does in the same frame, measuring the items
/// before it as needed.
#[derive(Clone)]
pub struct ScrollToCaret {
    /// The caret, in block-relative offsets.
    offset: usize,
    place: CaretPlace,
    /// The height of the window's document area, or zero before it's
    /// first drawn.
    viewport_height: Pixels,
    /// The item's bounds in this frame, once it's laid out.
    item: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl ScrollToCaret {
    pub fn new(offset: usize, place: CaretPlace, viewport_height: Pixels) -> Self {
        Self {
            offset,
            place,
            viewport_height,
            item: Rc::default(),
        }
    }

    /// Wraps the caret's item, so its bounds are known when the caret's
    /// element asks to be scrolled to.
    pub fn track_item(&self, item: AnyElement) -> AnyElement {
        let slot = self.item.clone();
        div()
            .relative()
            .child(
                canvas(move |bounds, _, _| slot.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(item)
            .into_any_element()
    }

    /// The space to ask the list to show, given where the caret's line
    /// is: a line above and below it, inside a window's height around the
    /// item if it has a place.
    fn bounds(&self, caret: Point<Pixels>, line: Pixels) -> Bounds<Pixels> {
        let (above, below) = (caret.y - line, caret.y + line * 2.);
        let height = self.viewport_height;
        let top = match (self.place, self.item.get()) {
            _ if height < below - above => None,
            (CaretPlace::Top(offset), Some(item)) => Some(item.top() - offset),
            (CaretPlace::Bottom, Some(item)) => Some(item.bottom() - height),
            _ => None,
        };
        match top {
            Some(top) => Bounds::new(
                point(caret.x, top.min(above).max(below - height)),
                size(px(CARET_WIDTH), height),
            ),
            None => Bounds::new(point(caret.x, above), size(px(CARET_WIDTH), below - above)),
        }
    }
}

/// The revealed block's text layout, in source offsets: tabs are shown as
/// spaces, so offsets in the layout differ from the source after a tab.
#[derive(Clone)]
pub struct RevealedLayout {
    layout: TextLayout,
    tabs: Rc<ExpandedTabs>,
}

impl RevealedLayout {
    /// Where the source `offset` is drawn.
    pub fn position_for_index(&self, offset: usize) -> Option<Point<Pixels>> {
        self.layout.position_for_index(self.tabs.to_shown(offset))
    }

    /// The source offset nearest `position`: `Ok` if it's over the text.
    /// A position over a tab's spaces is the tab.
    pub fn index_for_position(&self, position: Point<Pixels>) -> Result<usize, usize> {
        self.layout
            .index_for_position(position)
            .map(|shown| self.tabs.to_source(shown))
            .map_err(|shown| self.tabs.to_source(shown))
    }

    pub fn line_height(&self) -> Pixels {
        self.layout.line_height()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.layout.bounds()
    }
}

/// Renders a revealed block as a list item. `input` is set for the block
/// holding the caret; other blocks a selection reveals don't take input or
/// show the hint.
pub fn render_revealed(
    block: RevealedBlock,
    look: &Look,
    click: ClickTarget,
    input: Option<TextInput>,
) -> AnyElement {
    let heading = revealed_heading(block.styled);
    let first = block.first;
    let content = revealed_content(block, look, click.clone(), input);
    block_frame(content, heading, first, click)
}

/// Renders a block in rendered mode with the caret on an empty line in the
/// gap before or after it, where typing starts a new block.
pub fn render_gap(
    block: &RenderedBlock,
    gap: Gap,
    caret: RevealedBlock,
    look: &Look,
    click: ClickTarget,
    input: TextInput,
) -> AnyElement {
    let first = caret.first;
    let heading = rendered_heading(block);
    let rendered = rendered_content(block, look, click.clone());
    let line = revealed_content(caret, look, click.clone(), Some(input));
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
    look: &Look,
    click: ClickTarget,
    input: Option<TextInput>,
) -> AnyElement {
    let show_hint = input.as_ref().is_some_and(|input| input.hint);
    let heading = revealed_heading(block.styled);
    let monospace = block.styled.font == RevealFont::Monospace;

    let theme = look.theme;
    // Tabs are shown as spaces, as when rendered; the layout maps offsets
    // back to the source.
    let tabs = Rc::new(ExpandedTabs::new(block.source, look.tab_width));
    let text = SharedString::from(tabs.text.clone());
    let mut runs = text_runs(&block, look, heading);
    // GPUI requires the runs to cover the text exactly. If they ever don't,
    // show the source unstyled rather than crash.
    let covered: usize = runs.iter().map(|run| run.len).sum();
    if covered != block.source.len() {
        #[cfg(debug_assertions)]
        eprintln!(
            "KERTAS BUG: styled spans cover {covered} of {} bytes: {:?} spans {:?}",
            block.source.len(),
            block.source,
            block
                .styled
                .spans
                .iter()
                .map(|span| span.range.clone())
                .collect::<Vec<_>>()
        );
        runs = vec![TextRun {
            len: block.source.len(),
            font: look.font.clone(),
            color: theme.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }
    stretch_over_tabs(&mut runs, &tabs);
    let styled = StyledText::new(text).with_runs(runs);
    let layout = RevealedLayout {
        layout: styled.layout().clone(),
        tabs,
    };

    let caret = block.caret;
    let caret_layout = layout.clone();
    let scroll_layout = layout.clone();
    let scroll_to_caret = input
        .as_ref()
        .and_then(|input| input.scroll_to_caret.clone());
    let caret_overlay = canvas(
        // The text is laid out by now: ask the list to show the caret, in
        // this same frame.
        move |_, window, _| {
            if let Some(scroll) = scroll_to_caret
                && let Some(position) = scroll_layout.position_for_index(scroll.offset)
            {
                window.request_autoscroll(scroll.bounds(position, scroll_layout.line_height()));
            }
        },
        move |bounds, _, window, cx| {
            if let Some(input) = input {
                // Publish the layout only now that it's been painted: its
                // position queries panic before prepaint, and the list can
                // build an item just to measure it without painting it.
                *input.layout_slot.borrow_mut() = Some(caret_layout.clone());
                window.handle_input(
                    &input.focus_handle,
                    ElementInputHandler::new(bounds, input.view),
                    cx,
                );
            }
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
    let (drag_layout, drag) = (layout.clone(), click.clone());
    let text_element = div()
        .relative()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            click_text.report(index, event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                drag.drag(index, cx);
            }
        })
        .child(styled)
        .child(caret_overlay);

    let content = if monospace {
        code_text(div(), &look.code_font)
            .bg(theme.code_background)
            .rounded_md()
            .px_4()
            .py_3()
            .child(text_element)
    } else if let Some(level) = heading {
        heading_text(div(), level).child(text_element)
    } else {
        div().child(text_element)
    };

    // Highlight the revealed block. The highlight extends outside the text
    // column by its padding plus border, so the text doesn't shift. Clicks
    // on its padding count as clicks in the column.
    let hint = div()
        .absolute()
        .right(px(12.))
        .bottom(rems(-HINT_HEIGHT / 2.))
        .h(rems(HINT_HEIGHT))
        .px(px(6.))
        .bg(theme.background)
        .text_size(rems(HINT_TEXT_SIZE))
        .line_height(rems(HINT_HEIGHT))
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
        .when(show_hint, |this| this.child(hint))
        .into_any_element()
}

/// Text runs for the styled source, with the selection as a background.
fn text_runs(block: &RevealedBlock, look: &Look, heading: Option<u8>) -> Vec<TextRun> {
    let theme = look.theme;
    let mut block_font = look.font.clone();
    if matches!(block.styled.font, RevealFont::Monospace) {
        block_font.family = look.code_font.clone();
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
        let run = match span.style {
            SpanStyle::Syntax => inline_run(
                InlineStyle::default(),
                0,
                &block_font,
                theme.muted_text,
                theme,
                &look.code_font,
            ),
            SpanStyle::Content(mut inline) => {
                // A code block is already in the code font, unhighlighted.
                inline.code &= block.styled.font != RevealFont::Monospace;
                inline_run(inline, 0, &block_font, base_color, theme, &look.code_font)
            }
        };

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
            let mut piece_run = TextRun {
                len: piece.len(),
                ..run.clone()
            };
            if within(&marked) {
                piece_run.underline = Some(underline(run.color));
            }
            if within(&block.selection) {
                piece_run.background_color = Some(theme.selection);
            }
            runs.push(piece_run);
        }
    }
    runs
}

/// Lengthens `runs`, which measure the source, to cover its tabs shown as
/// spaces.
fn stretch_over_tabs(runs: &mut [TextRun], tabs: &ExpandedTabs) {
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        run.len = tabs.to_shown(end) - tabs.to_shown(start);
        start = end;
    }
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
    use crate::theme::Theme;
    use kertas_core::model::Model;
    use kertas_core::reveal::styled_source;

    /// GPUI requires a revealed block's text runs to cover its text
    /// exactly, whatever is selected or being composed (T2).
    #[test]
    fn text_runs_cover_the_text() {
        let look = Look {
            font: gpui::font("Helvetica"),
            code_font: "Menlo".into(),
            theme: Theme::light(),
            tab_width: 4,
            list_indent: kertas_core::settings::ListIndent::Standard,
            image_scale: 1.,
        };
        for name in ["showcase.md", "10k.md"] {
            let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
            let text = std::fs::read_to_string(path).unwrap();
            let model = Model::new(&text);
            for (index, item) in model.items.iter().enumerate() {
                for range in [model.source_range(index), item.range.clone()] {
                    let source = &text[range.clone()];
                    let styled = styled_source(source, item.kind, &model.definitions);
                    let heading = revealed_heading(&styled);
                    let check = |selection: Range<usize>, marked: Option<Range<usize>>| {
                        let block = RevealedBlock {
                            source,
                            styled: &styled,
                            selection: selection.clone(),
                            caret: None,
                            marked: marked.clone(),
                            first: false,
                        };
                        let covered: usize = text_runs(&block, &look, heading)
                            .iter()
                            .map(|run| run.len)
                            .sum();
                        assert_eq!(
                            covered,
                            source.len(),
                            "{name} item {index} {range:?}, selection {selection:?}, \
                             marked {marked:?}"
                        );
                    };
                    // Runs are only cut where the selection or marked text
                    // starts or ends, so what matters is where those fall
                    // relative to the spans: on, just before or just after
                    // each span's edges.
                    let mut offsets: Vec<usize> = styled
                        .spans
                        .iter()
                        .flat_map(|span| [span.range.start, span.range.end])
                        .chain([0, source.len()])
                        .flat_map(|edge| [edge.saturating_sub(1), edge, edge + 1])
                        .filter(|&offset| offset <= source.len() && source.is_char_boundary(offset))
                        .collect();
                    offsets.sort_unstable();
                    offsets.dedup();
                    // Every pair in small blocks; in large ones, every caret
                    // and every selection to or from either end.
                    let every_pair = offsets.len() <= 60;
                    for (i, &start) in offsets.iter().enumerate() {
                        if every_pair {
                            for &end in &offsets[i..] {
                                check(start..end, None);
                            }
                        } else {
                            check(start..start, None);
                            check(0..start, None);
                            check(start..source.len(), None);
                        }
                        let end = offsets[(i + 2).min(offsets.len() - 1)];
                        check(start..start, Some(start..end));
                        check(0..source.len(), Some(start..end));
                    }
                }
            }
        }
    }

    #[test]
    fn runs_stretch_over_tabs() {
        let tabs = ExpandedTabs::new("\ta\tb", 4);
        let run = |len| TextRun {
            len,
            font: gpui::font("Helvetica"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // "\t" | "a\t" | "b"
        let mut runs = vec![run(1), run(2), run(1)];
        stretch_over_tabs(&mut runs, &tabs);
        let lens: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lens, [4, 5, 1]);
        assert_eq!(lens.iter().sum::<usize>(), tabs.text.len());
    }

    #[test]
    fn splits_at_cuts_inside_range() {
        assert_eq!(split_at(0..10, &[3, 6]), [0..3, 3..6, 6..10]);
        assert_eq!(split_at(0..10, &[0, 10]), vec![0..10]);
        assert_eq!(split_at(0..10, &[8, 20]), [0..8, 8..10]);
        assert_eq!(split_at(5..10, &[0, 2]), vec![5..10]);
        assert_eq!(split_at(0..10, &[4, 4, 6, 2]), [0..2, 2..4, 4..6, 6..10]);
    }
}
