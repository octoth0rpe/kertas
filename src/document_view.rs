//! The view that shows a document in a window.
//!
//! Paragraphs, headings and thematic breaks are rendered; other blocks are
//! shown as their source text until they're supported.

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
use kami::rendered::{RenderedBlock, render_blocks};

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

/// Renders one block as a list item: the block centered in the text
/// column, with spacing around it.
fn render_block(block: &RenderedBlock, first: bool, base_font: &Font) -> AnyElement {
    let column = div().w_full().max_w(px(MAX_TEXT_WIDTH)).mx_auto();
    let content = match block {
        RenderedBlock::Paragraph(text) => column.child(styled_text(text, base_font)),
        RenderedBlock::Heading { level, text } => {
            let (size, top) = match level {
                1 => (2.0, 24.),
                2 => (1.5, 24.),
                3 => (1.25, 20.),
                4 => (1.0, 16.),
                5 => (0.875, 16.),
                _ => (0.85, 16.),
            };
            let mut font = base_font.clone();
            font.weight = FontWeight::SEMIBOLD;
            column
                .when(!first, |this| this.pt(px(top)))
                .text_size(px(BODY_TEXT_SIZE * size))
                .line_height(relative(1.25))
                .when(*level == 6, |this| this.text_color(rgb(MUTED_TEXT)))
                .child(styled_text(text, &font))
        }
        RenderedBlock::ThematicBreak => column.py(px(8.)).child(div().h(px(2.)).bg(rgb(RULE))),
        RenderedBlock::Raw(source) => column
            .font_family(CODE_FONT)
            .text_size(px(13.))
            .line_height(relative(1.5))
            .text_color(rgb(MUTED_TEXT))
            .child(source.replace('\t', "    ")),
    };
    div()
        .w_full()
        .px_8()
        .when(first, |this| this.pt(px(32.)))
        .pb(px(16.))
        .child(content)
        .into_any_element()
}

/// Inline text with its formatting as GPUI text runs.
fn styled_text(text: &InlineText, base_font: &Font) -> StyledText {
    let runs = text
        .segments
        .iter()
        .map(|segment| {
            let style = segment.style;
            let mut font = if style.code {
                Font {
                    family: CODE_FONT.into(),
                    ..base_font.clone()
                }
            } else {
                base_font.clone()
            };
            if style.strong {
                font.weight = FontWeight::BOLD;
            }
            if style.emphasis {
                font.style = FontStyle::Italic;
            }
            let color: Hsla = if style.link { rgb(LINK) } else { rgb(TEXT) }.into();
            TextRun {
                len: segment.display.len(),
                font,
                color,
                background_color: style.code.then(|| rgb(CODE_BACKGROUND).into()),
                underline: style.link.then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: Some(color),
                    wavy: false,
                }),
                strikethrough: style.strikethrough.then(|| StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(color),
                }),
            }
        })
        .collect();
    StyledText::new(SharedString::from(text.text.clone())).with_runs(runs)
}
