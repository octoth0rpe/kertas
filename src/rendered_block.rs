//! Drawing blocks in rendered mode, and mapping clicks on them back to
//! source offsets.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Font, FontWeight, Hsla, MouseButton, Rems, SharedString, StyledText,
    WeakEntity, Window, div, prelude::*, px, rems,
};
use kami::inline::InlineText;
use kami::rendered::{ColumnAlignment, ListItem, RenderedBlock, Table};
use kami::settings::ListIndent;
use kami::tabs::ExpandedTabs;

use crate::document_view::DocumentView;
use crate::text_style::{Look, code_text, heading_text, inline_run};
use crate::theme::Theme;

/// Space between blocks inside lists and blockquotes.
const NESTED_GAP: f32 = 8.;

/// The width of a list's marker column with the standard `list_indent`,
/// in rems (28 px at the default size): how far its items are indented.
const MARKER_COLUMN: f32 = 1.75;

/// Space between a list marker and its item, in rems.
const MARKER_GAP: f32 = 0.5;

/// Text styling inherited by nested blocks.
#[derive(Clone)]
struct TextStyle {
    font: Font,
    color: Hsla,
    theme: Theme,
    tab_width: u32,
    list_indent: ListIndent,
    code_font: SharedString,
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
    look: &Look,
    click: ClickTarget,
) -> AnyElement {
    let content = rendered_content(block, look, click.clone());
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
pub fn rendered_content(block: &RenderedBlock, look: &Look, click: ClickTarget) -> AnyElement {
    let style = TextStyle {
        font: look.font.clone(),
        color: look.theme.text,
        theme: look.theme,
        tab_width: look.tab_width,
        list_indent: look.list_indent,
        code_font: look.code_font.clone(),
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
        RenderedBlock::CodeBlock { code, .. } => code_text(div(), &style.code_font)
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
        RenderedBlock::Html(source) | RenderedBlock::Raw(source) => {
            code_text(div(), &style.code_font)
                .text_color(style.theme.muted_text)
                .child(clickable_source(source, style, |index| index))
                .into_any_element()
        }
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
    let markers: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| match (item.task, start) {
            (Some(true), _) => "☑".to_owned(),
            (Some(false), _) => "☐".to_owned(),
            (None, Some(start)) => format!("{}.", start + index as u64),
            (None, None) => bullet.to_owned(),
        })
        .collect();
    let column = marker_column(style.list_indent, &markers);
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(items.iter().zip(markers).map(|(item, marker)| {
            div()
                .flex()
                .child(
                    div()
                        .flex_none()
                        .w(rems(column))
                        .pr(rems(MARKER_GAP))
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

/// The width of a list's marker column, in rems: the `list_indent`, or
/// wider if a marker (such as "100.") needs more room.
fn marker_column(list_indent: ListIndent, markers: &[String]) -> f32 {
    let widest = markers
        .iter()
        .map(|marker| marker_width(marker))
        .fold(0., f32::max);
    (MARKER_COLUMN * list_indent.scale()).max(widest + MARKER_GAP)
}

/// About how wide `marker` is, in ems, erring on the wide side.
fn marker_width(marker: &str) -> f32 {
    marker
        .chars()
        .map(|c| match c {
            '.' => 0.3,
            '☐' | '☑' => 1.,
            _ => 0.6,
        })
        .sum()
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
    let tabs = ExpandedTabs::new(text, style.tab_width);
    let styled = StyledText::new(SharedString::from(tabs.text.clone()));
    let layout = styled.layout().clone();
    let click = style.click.clone();
    let to_source = Rc::new(move |shown: usize| to_source(tabs.to_source(shown)));
    let (drag_layout, drag_to_source, drag) = (layout.clone(), to_source.clone(), click.clone());
    div()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            click.report(to_source(index), event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                drag.drag(drag_to_source(index), cx);
            }
        })
        .child(styled)
        .into_any_element()
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
                &style.code_font,
            )
        })
        .collect();
    StyledText::new(SharedString::from(text.text.clone())).with_runs(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markers(markers: &[&str]) -> Vec<String> {
        markers.iter().map(|&marker| marker.to_owned()).collect()
    }

    #[test]
    fn marker_column_follows_the_list_indent() {
        let bullets = markers(&["•", "•"]);
        assert_eq!(marker_column(ListIndent::Standard, &bullets), 1.75);
        assert_eq!(marker_column(ListIndent::Compact, &bullets), 1.3125);
        assert_eq!(marker_column(ListIndent::Wide, &bullets), 2.625);
    }

    #[test]
    fn marker_column_widens_for_long_markers() {
        let short = markers(&["8.", "9."]);
        assert_eq!(marker_column(ListIndent::Standard, &short), 1.75);
        let long = markers(&["99.", "100."]);
        assert!(marker_column(ListIndent::Standard, &long) > 2.5);
        assert!(marker_column(ListIndent::Compact, &markers(&["☐"])) >= 1.5);
    }
}
