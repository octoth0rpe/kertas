//! The view that shows a document in a window.
//!
//! Every block is rendered except front matter and link reference
//! definitions, which are shown as their source text.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, FocusHandle, Font, FontStyle, FontWeight, Hsla, ListAlignment, ListState,
    PathPromptOptions, PromptLevel, SharedString, StrikethroughStyle, StyledText, TextRun,
    UnderlineStyle, Window, actions, div, list, prelude::*, px, relative, rgb,
};
use kami::blocks::Blocks;
use kami::document::{Document, OpenError};
use kami::inline::InlineText;
use kami::rendered::{ColumnAlignment, ListItem, RenderedBlock, Table, render_blocks};

actions!(kami, [Open, CloseWindow]);

/// Maximum width of the text column (A1).
const MAX_TEXT_WIDTH: f32 = 720.;
const BODY_TEXT_SIZE: f32 = 16.;
const CODE_FONT: &str = "Menlo";

// Hardcoded light colors; themes come in a later story.
const BACKGROUND: u32 = 0xffffff;
const TEXT: u32 = 0x1f2328;
const MUTED_TEXT: u32 = 0x656d76;
const LINK: u32 = 0x0969da;
const CODE_BACKGROUND: u32 = 0xeff1f3;
const RULE: u32 = 0xd1d9e0;
const TABLE_HEADER_BACKGROUND: u32 = 0xf6f8fa;

/// Space between blocks inside lists and blockquotes.
const NESTED_GAP: f32 = 8.;

/// An open document and everything needed to display it.
struct OpenDocument {
    #[expect(dead_code, reason = "read by the save and editing stories")]
    document: Document,
    blocks: Rc<Vec<RenderedBlock>>,
    list_state: ListState,
}

pub struct DocumentView {
    focus_handle: FocusHandle,
    open: Option<OpenDocument>,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            focus_handle,
            open: None,
        }
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match Document::open(path) {
            Ok(document) => {
                window.set_window_title(&document.display_name());
                let text = document.buffer().rope().to_string();
                let blocks = render_blocks(&text, &Blocks::parse(&text));
                let list_state = ListState::new(blocks.len(), ListAlignment::Top, px(1000.));
                self.open = Some(OpenDocument {
                    document,
                    blocks: Rc::new(blocks),
                    list_state,
                });
                cx.notify();
            }
            Err(err) => show_open_error(&err, window, cx),
        }
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            // The outer error means the dialog was dropped, the inner one that
            // the platform couldn't show it; `None` means the user cancelled.
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.open_path(path, window, cx))
                .ok();
        })
        .detach();
    }
}

fn show_open_error(err: &OpenError, window: &mut Window, cx: &mut Context<DocumentView>) {
    let message = format!("Couldn't open “{}”", err.display_name());
    let detail = capitalize(&err.source.to_string());
    let answer = window.prompt(PromptLevel::Critical, &message, Some(&detail), &["OK"], cx);
    cx.spawn(async move |_, _| answer.await.ok()).detach();
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.open {
            Some(open) => {
                let blocks = open.blocks.clone();
                let base_font = window.text_style().font();
                list(open.list_state.clone(), move |index, _, _| {
                    render_block(&blocks[index], index == 0, &base_font)
                })
                .size_full()
                .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(MUTED_TEXT))
                .child("Open a Markdown file with ⌘O")
                .into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_size(px(BODY_TEXT_SIZE))
            .line_height(relative(1.6))
            .child(content)
    }
}

/// Text styling inherited by nested blocks.
#[derive(Clone)]
struct TextStyle {
    font: Font,
    color: Hsla,
}

/// Renders one block as a list item: the block centered in the text
/// column, with spacing around it.
fn render_block(block: &RenderedBlock, first: bool, base_font: &Font) -> AnyElement {
    let style = TextStyle {
        font: base_font.clone(),
        color: rgb(TEXT).into(),
    };
    let top = match block {
        RenderedBlock::Heading { level, .. } if !first => heading_top_space(*level),
        _ => 0.,
    };
    div()
        .w_full()
        .px_8()
        .when(first, |this| this.pt(px(32.)))
        .pb(px(16.))
        .child(
            div()
                .w_full()
                .max_w(px(MAX_TEXT_WIDTH))
                .mx_auto()
                .pt(px(top))
                .child(render_content(block, &style, 0)),
        )
        .into_any_element()
}

/// Renders a block's content, without the spacing around it. `depth` is
/// how many lists the block is nested in, which picks the bullet style.
fn render_content(block: &RenderedBlock, style: &TextStyle, depth: usize) -> AnyElement {
    match block {
        RenderedBlock::Paragraph(text) => div().child(styled_text(text, style)).into_any_element(),
        RenderedBlock::Heading { level, text } => {
            let size = match level {
                1 => 2.0,
                2 => 1.5,
                3 => 1.25,
                4 => 1.0,
                5 => 0.875,
                _ => 0.85,
            };
            let mut heading_style = style.clone();
            heading_style.font.weight = FontWeight::SEMIBOLD;
            if *level == 6 {
                heading_style.color = rgb(MUTED_TEXT).into();
            }
            div()
                .text_size(px(BODY_TEXT_SIZE * size))
                .line_height(relative(1.25))
                .child(styled_text(text, &heading_style))
                .into_any_element()
        }
        RenderedBlock::ThematicBreak => div()
            .py(px(8.))
            .child(div().h(px(2.)).bg(rgb(RULE)))
            .into_any_element(),
        RenderedBlock::List { start, items } => render_list(*start, items, style, depth),
        RenderedBlock::BlockQuote(children) => {
            let quote_style = TextStyle {
                color: rgb(MUTED_TEXT).into(),
                ..style.clone()
            };
            div()
                .border_l_4()
                .border_color(rgb(RULE))
                .pl_4()
                .text_color(rgb(MUTED_TEXT))
                .child(render_children(children, &quote_style, depth))
                .into_any_element()
        }
        RenderedBlock::CodeBlock { code, .. } => div()
            .bg(rgb(CODE_BACKGROUND))
            .rounded_md()
            .px_4()
            .py_3()
            .font_family(CODE_FONT)
            .text_size(px(13.))
            .line_height(relative(1.5))
            .text_color(rgb(TEXT))
            .child(code.trim_end_matches(['\n', '\r']).replace('\t', "    "))
            .into_any_element(),
        RenderedBlock::Table(table) => render_table(table, style),
        RenderedBlock::Html(source) | RenderedBlock::Raw(source) => div()
            .font_family(CODE_FONT)
            .text_size(px(13.))
            .line_height(relative(1.5))
            .text_color(rgb(MUTED_TEXT))
            .child(source.replace('\t', "    "))
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
            .when(header, |this| this.bg(rgb(TABLE_HEADER_BACKGROUND)))
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
                    .border_color(rgb(RULE))
                    .map(|this| match alignment {
                        ColumnAlignment::Center => this.text_center(),
                        ColumnAlignment::Right => this.text_right(),
                        ColumnAlignment::Left | ColumnAlignment::None => this,
                    })
                    .child(styled_text(cell, style))
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

/// Inline text with its formatting as GPUI text runs.
fn styled_text(text: &InlineText, style: &TextStyle) -> StyledText {
    let runs = text
        .segments
        .iter()
        .map(|segment| {
            let inline = segment.style;
            let mut font = if inline.code {
                Font {
                    family: CODE_FONT.into(),
                    ..style.font.clone()
                }
            } else {
                style.font.clone()
            };
            if inline.strong {
                font.weight = FontWeight::BOLD;
            }
            if inline.emphasis {
                font.style = FontStyle::Italic;
            }
            let color: Hsla = if inline.link {
                rgb(LINK).into()
            } else {
                style.color
            };
            TextRun {
                len: segment.display.len(),
                font,
                color,
                background_color: inline.code.then(|| rgb(CODE_BACKGROUND).into()),
                underline: inline.link.then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: Some(color),
                    wavy: false,
                }),
                strikethrough: inline.strikethrough.then(|| StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(color),
                }),
            }
        })
        .collect();
    StyledText::new(SharedString::from(text.text.clone())).with_runs(runs)
}
