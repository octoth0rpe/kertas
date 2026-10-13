//! Drawing blocks in rendered mode, and mapping clicks on them back to
//! source offsets.

use gpui::{
    AnyElement, App, Font, FontWeight, Hsla, MouseButton, Rems, SharedString, StyledText,
    WeakEntity, Window, div, prelude::*, px,
};
use kami::inline::InlineText;
use kami::rendered::{ColumnAlignment, ListItem, RenderedBlock, Table};

use crate::document_view::DocumentView;
use crate::text_style::{code_text, heading_text, inline_run};
use crate::theme::Theme;

/// Space between blocks inside lists and blockquotes.
const NESTED_GAP: f32 = 8.;

/// Text styling inherited by nested blocks.
#[derive(Clone)]
struct TextStyle {
    font: Font,
    color: Hsla,
    theme: Theme,
    click: ClickTarget,
}

/// Where clicks on a block's text are reported, and the text column the
/// block is drawn in.
#[derive(Clone)]
pub struct ClickTarget {
    pub view: WeakEntity<DocumentView>,
    /// The block's start offset in the document.
    pub block_start: usize,
    /// The text column's maximum width, or `None` for the full window.
    pub column_width: Option<Rems>,
}

impl ClickTarget {
    /// Places the caret at `offset` in the block's source (shift-click
    /// extends the selection).
    pub fn report(&self, offset: usize, extend: bool, window: &mut Window, cx: &mut App) {
        let offset = self.block_start + offset;
        self.view
            .update(cx, |view, cx| view.click(offset, extend, window, cx))
            .ok();
    }

    /// Extends the selection to `offset` in the block's source while the
    /// mouse is dragged.
    pub fn drag(&self, offset: usize, cx: &mut App) {
        let offset = self.block_start + offset;
        self.view
            .update(cx, |view, cx| view.drag_to(offset, cx))
            .ok();
    }

    /// Finishes editing, as Escape does.
    pub fn finish_editing(&self, cx: &mut App) {
        self.view
            .update(cx, |view, cx| view.finish_editing(cx))
            .ok();
    }
}

/// Renders one block in rendered mode as a list item.
pub fn render_block(
    block: &RenderedBlock,
    first: bool,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
) -> AnyElement {
    let content = rendered_content(block, base_font, theme, click.clone());
    block_frame(content, rendered_heading(block), first, click)
}

/// The heading level of a rendered block, if it's a heading.
pub fn rendered_heading(block: &RenderedBlock) -> Option<u8> {
    match block {
        RenderedBlock::Heading { level, .. } => Some(*level),
        _ => None,
    }
}

/// A block's rendered content, without the list item's spacing.
pub fn rendered_content(
    block: &RenderedBlock,
    base_font: &Font,
    theme: Theme,
    click: ClickTarget,
) -> AnyElement {
    let style = TextStyle {
        font: base_font.clone(),
        color: theme.text,
        theme,
        click,
    };
    render_content(block, &style, 0)
}

/// Wraps a block's content as a list item: centered in the text column,
/// with spacing around it. Clicks in the column that miss the block's text
/// place the caret at the block's start; clicks outside the column finish
/// editing.
pub fn block_frame(
    content: AnyElement,
    heading: Option<u8>,
    first: bool,
    click: ClickTarget,
) -> AnyElement {
    let top = match heading {
        Some(level) if !first => heading_top_space(level),
        _ => 0.,
    };
    let outside = click.clone();
    let column_width = click.column_width;
    div()
        .w_full()
        .px_8()
        .when(first, |this| this.pt(px(32.)))
        .pb(px(16.))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            outside.finish_editing(cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .w_full()
                .when_some(column_width, |this, width| this.max_w(width))
                .mx_auto()
                .pt(px(top))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    click.report(0, event.modifiers.shift, window, cx);
                    cx.stop_propagation();
                })
                .child(content),
        )
        .into_any_element()
}

/// Renders a block's content, without the spacing around it. `depth` is
/// how many lists the block is nested in, which picks the bullet style.
fn render_content(block: &RenderedBlock, style: &TextStyle, depth: usize) -> AnyElement {
    match block {
        RenderedBlock::Paragraph(text) => clickable_text(text, style),
        RenderedBlock::Heading { level, text } => {
            let mut heading_style = style.clone();
            heading_style.font.weight = FontWeight::SEMIBOLD;
            if *level == 6 {
                heading_style.color = style.theme.muted_text;
            }
            heading_text(div(), *level)
                .child(clickable_text(text, &heading_style))
                .into_any_element()
        }
        RenderedBlock::ThematicBreak => div()
            .py(px(8.))
            .child(div().h(px(2.)).bg(style.theme.rule))
            .into_any_element(),
        RenderedBlock::List { start, items } => render_list(*start, items, style, depth),
        RenderedBlock::BlockQuote(children) => {
            let quote_style = TextStyle {
                color: style.theme.muted_text,
                ..style.clone()
            };
            div()
                .border_l_4()
                .border_color(style.theme.rule)
                .pl_4()
                .text_color(style.theme.muted_text)
                .child(render_children(children, &quote_style, depth))
                .into_any_element()
        }
        RenderedBlock::CodeBlock { code, .. } => code_text(div())
            .bg(style.theme.code_background)
            .rounded_md()
            .px_4()
            .py_3()
            .text_color(style.theme.text)
            .child({
                let shown = code.text.trim_end_matches(['\n', '\r']);
                let code = code.clone();
                clickable_source(shown, style, move |index| code.source_offset(index))
            })
            .into_any_element(),
        RenderedBlock::Table(table) => render_table(table, style),
        RenderedBlock::Html(source) | RenderedBlock::Raw(source) => code_text(div())
            .text_color(style.theme.muted_text)
            .child(clickable_source(source, style, |index| index))
            .into_any_element(),
    }
}

/// Nested blocks stacked vertically, e.g. the contents of a list item.
fn render_children(children: &[RenderedBlock], style: &TextStyle, depth: usize) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(NESTED_GAP))
        .children(
            children
                .iter()
                .map(|child| render_content(child, style, depth)),
        )
        .into_any_element()
}

fn render_list(
    start: Option<u64>,
    items: &[ListItem],
    style: &TextStyle,
    depth: usize,
) -> AnyElement {
    let bullet = ["•", "◦", "▪"][depth % 3];
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(items.iter().enumerate().map(|(index, item)| {
            let marker = match (item.task, start) {
                (Some(true), _) => "☑".to_owned(),
                (Some(false), _) => "☐".to_owned(),
                (None, Some(start)) => format!("{}.", start + index as u64),
                (None, None) => bullet.to_owned(),
            };
            div()
                .flex()
                .child(
                    div()
                        .flex_none()
                        .w(px(28.))
                        .pr(px(8.))
                        .text_right()
                        .text_color(style.color)
                        .child(marker),
                )
                .child(div().flex_1().min_w(px(0.)).child(render_children(
                    &item.children,
                    style,
                    depth + 1,
                )))
        }))
        .into_any_element()
}

fn render_table(table: &Table, style: &TextStyle) -> AnyElement {
    let mut header_style = style.clone();
    header_style.font.weight = FontWeight::SEMIBOLD;

    let row = |cells: &[InlineText], style: &TextStyle, header: bool| {
        div()
            .flex()
            .when(header, |this| this.bg(style.theme.table_header_background))
            .children(cells.iter().enumerate().map(|(column, cell)| {
                let alignment = table
                    .alignments
                    .get(column)
                    .copied()
                    .unwrap_or(ColumnAlignment::None);
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .px_3()
                    .py_1()
                    .border_1()
                    .border_color(style.theme.rule)
                    .map(|this| match alignment {
                        ColumnAlignment::Center => this.text_center(),
                        ColumnAlignment::Right => this.text_right(),
                        ColumnAlignment::Left | ColumnAlignment::None => this,
                    })
                    .child(clickable_text(cell, style))
            }))
    };

    div()
        .flex()
        .flex_col()
        .child(row(&table.header, &header_style, true))
        .children(table.rows.iter().map(|cells| row(cells, style, false)))
        .into_any_element()
}

fn heading_top_space(level: u8) -> f32 {
    match level {
        1 | 2 => 24.,
        3 => 20.,
        _ => 16.,
    }
}

/// Inline text that places the caret at the matching source position
/// when clicked.
fn clickable_text(text: &InlineText, style: &TextStyle) -> AnyElement {
    let styled = styled_text(text, style);
    let layout = styled.layout().clone();
    let inline = text.clone();
    let click = style.click.clone();
    let (drag_layout, drag_inline, drag) = (layout.clone(), inline.clone(), click.clone());
    div()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            click.report(
                inline.source_offset(index),
                event.modifiers.shift,
                window,
                cx,
            );
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                drag.drag(drag_inline.source_offset(index), cx);
            }
        })
        .child(styled)
        .into_any_element()
}

/// Plain text shown with tabs expanded. A click maps the clicked index back
/// through `to_source` (which takes an index into `text`).
fn clickable_source(
    text: &str,
    style: &TextStyle,
    to_source: impl Fn(usize) -> usize + 'static,
) -> AnyElement {
    let original = text.to_owned();
    let styled = StyledText::new(SharedString::from(expand_tabs(text)));
    let layout = styled.layout().clone();
    let click = style.click.clone();
    let to_source = std::rc::Rc::new(to_source);
    let (drag_layout, drag_original, drag_to_source, drag) = (
        layout.clone(),
        original.clone(),
        to_source.clone(),
        click.clone(),
    );
    div()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            let index = unexpand_tabs(&original, index);
            click.report(to_source(index), event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                let index = unexpand_tabs(&drag_original, index);
                drag.drag(drag_to_source(index), cx);
            }
        })
        .child(styled)
        .into_any_element()
}

/// How a tab is displayed.
const TAB: &str = "    ";

pub fn expand_tabs(text: &str) -> String {
    text.replace('\t', TAB)
}

/// Maps an index in `text` with tabs expanded to [`TAB`] back to an index in
/// `text`. An index inside an expanded tab maps to the tab's start.
fn unexpand_tabs(text: &str, expanded: usize) -> usize {
    let mut shown = 0;
    for (index, ch) in text.char_indices() {
        let width = if ch == '\t' { TAB.len() } else { ch.len_utf8() };
        if expanded < shown + width {
            return index;
        }
        shown += width;
    }
    text.len()
}

/// Inline text with its formatting as GPUI text runs.
fn styled_text(text: &InlineText, style: &TextStyle) -> StyledText {
    let runs = text
        .segments
        .iter()
        .map(|segment| {
            inline_run(
                segment.style,
                segment.display.len(),
                &style.font,
                style.color,
                style.theme,
            )
        })
        .collect();
    StyledText::new(SharedString::from(text.text.clone())).with_runs(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpand_tabs_maps_back() {
        let text = "a\tb";
        // Shown as "a    b": a(0) tab(1..5) b(5).
        assert_eq!(unexpand_tabs(text, 0), 0);
        assert_eq!(unexpand_tabs(text, 1), 1);
        assert_eq!(unexpand_tabs(text, 4), 1, "inside the tab");
        assert_eq!(unexpand_tabs(text, 5), 2);
        assert_eq!(unexpand_tabs(text, 6), 3, "end");
        assert_eq!(unexpand_tabs("no tabs", 3), 3);
    }
}
