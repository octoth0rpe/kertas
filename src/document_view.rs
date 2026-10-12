//! The view that shows a document in a window.
//!
//! For now it shows the document's raw text, read-only. Rendering comes in
//! a later story.

use std::path::PathBuf;

use gpui::{
    Context, FocusHandle, PathPromptOptions, PromptLevel, SharedString, Window, actions, div,
    prelude::*, px, rgb, uniform_list,
};
use kami::document::{Document, OpenError};
use ropey::Rope;

actions!(kami, [Open, CloseWindow]);

const LINE_HEIGHT: f32 = 20.;

pub struct DocumentView {
    focus_handle: FocusHandle,
    document: Option<Document>,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            focus_handle,
            document: None,
        }
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match Document::open(path) {
            Ok(document) => {
                window.set_window_title(&document.display_name());
                self.document = Some(document);
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

/// The text of line `index`, without its line ending, ready to display.
fn display_line(rope: &Rope, index: usize) -> SharedString {
    let line = rope.line(index).to_string();
    let line = line.trim_end_matches(['\n', '\r']);
    line.replace('\t', "    ").into()
}

impl Render for DocumentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.document {
            Some(document) => {
                let rope = document.buffer().rope().clone();
                uniform_list("document-lines", rope.len_lines(), move |range, _, _| {
                    range
                        .map(|index| {
                            div()
                                .h(px(LINE_HEIGHT))
                                .whitespace_nowrap()
                                .child(display_line(&rope, index))
                        })
                        .collect()
                })
                .size_full()
                .px_4()
                .py_2()
                .font_family("Menlo")
                .text_size(px(13.))
                .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x8a8a8a))
                .child("Open a Markdown file with ⌘O")
                .into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .child(content)
    }
}
