//! Opening, saving and closing documents, and printing.

use std::path::{Path, PathBuf};

use crate::rendered_block::ReferenceTarget;
use gpui::{Context, ListOffset, PromptLevel, Task, Window, px};
use kertas::anchors;
use kertas::document::{Document, OpenError, SaveError};
use kertas::links::{self, LinkTarget};

use super::{CloseWindow, DocumentView, Print, Save, SaveAs};
use crate::builtin::Builtin;

impl DocumentView {
    /// File > Print…: the standard print panel, as a sheet on this window.
    pub(super) fn print(&mut self, _: &Print, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            let Some(open) = &self.open else {
                return;
            };
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            let Some(ns_window) = crate::native_tabs::ns_window(window) else {
                return;
            };
            let title = match self.builtin {
                Some(builtin) => builtin.title().to_owned(),
                None => open.editor.document().display_name(),
            };
            let settings = crate::app_settings::get(cx);
            crate::print::print(
                open.editor.model(),
                open.editor.document().path(),
                &settings,
                &title,
                &ns_window,
                mtm,
            );
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (window, cx);
    }

    /// Shows one of Kertas's built-in documents, view-only.
    pub fn open_builtin(&mut self, builtin: Builtin, window: &mut Window, cx: &mut Context<Self>) {
        self.builtin = Some(builtin);
        self.load(Document::from_text(&builtin.text()), window, cx);
    }

    /// The built-in document shown, if any.
    pub fn builtin(&self) -> Option<Builtin> {
        self.builtin
    }

    /// Starts a new untitled document, ready for typing.
    pub fn start_untitled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load(Document::untitled(), window, cx);
        self.move_caret(0, false, cx);
    }

    /// The open document's file, if it has one.
    pub fn document_path(&self) -> Option<&Path> {
        self.open.as_ref()?.editor.document().path()
    }

    /// Whether the window shows the startup message rather than a document.
    /// New starts its document in such a window instead of adding a tab.
    pub fn is_blank(&self) -> bool {
        self.builtin.is_none() && self.open.is_none()
    }

    /// Whether the window holds nothing worth keeping: no document, or an
    /// untitled one that's still empty. Opening a file reuses such a window
    /// instead of adding a tab.
    pub fn is_pristine(&self) -> bool {
        self.builtin.is_none()
            && self.open.as_ref().is_none_or(|open| {
                let document = open.editor.document();
                document.path().is_none() && document.buffer().is_empty()
            })
    }

    pub(super) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.save_document(false, window, cx).detach();
    }

    pub(super) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_document(true, window, cx).detach();
    }

    /// Saves the document, asking for a location if it's untitled or
    /// `choose_path` is set. Resolves to whether it was saved.
    fn save_document(
        &mut self,
        choose_path: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let Some(open) = &mut self.open else {
            return Task::ready(false);
        };
        // Read-only documents have nothing to save.
        if open.editor.read_only() {
            return Task::ready(false);
        }
        let current_path = open.editor.document().path().map(Path::to_owned);
        if !choose_path && current_path.is_some() {
            let result = open.editor.save();
            return Task::ready(self.finish_save(result, window, cx));
        }

        let directory = current_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let suggested = match &current_path {
            Some(_) => open.editor.document().display_name(),
            None => "Untitled.md".to_owned(),
        };
        let path = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return false;
            };
            this.update_in(cx, |this, window, cx| {
                let Some(open) = &mut this.open else {
                    return false;
                };
                let result = open.editor.save_as(path);
                this.finish_save(result, window, cx)
            })
            .unwrap_or(false)
        })
    }

    /// Updates the window after a save, or shows why it failed.
    fn finish_save(
        &mut self,
        result: Result<(), SaveError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            Ok(()) => {
                if let Some(open) = &mut self.open {
                    open.disk_change = None;
                    open.autosave_paused = false;
                }
                // Save As moves the document to another file.
                self.watch_file(window, cx);
                self.update_window_title(window, cx);
                cx.notify();
                true
            }
            Err(err) => {
                show_save_error(&err, window, cx);
                false
            }
        }
    }

    /// Asks whether to save unsaved changes before they'd be lost. Resolves
    /// to whether it's OK to continue: there were no changes, they were
    /// saved, or the user chose not to save them.
    fn confirm_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        if !self.is_dirty() {
            return Task::ready(true);
        }
        if self.confirming {
            return Task::ready(false);
        }
        self.confirming = true;
        let name = self
            .open
            .as_ref()
            .map(|open| open.editor.document().display_name())
            .unwrap_or_default();
        let message = format!("Do you want to save the changes you made to “{name}”?");
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            Some("Your changes will be lost if you don't save them."),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let save = this
                .update_in(cx, |this, window, cx| {
                    this.confirming = false;
                    match answer {
                        Some(0) => Some(this.save_document(false, window, cx)),
                        _ => None,
                    }
                })
                .ok()
                .flatten();
            match (answer, save) {
                (Some(0), Some(save)) => save.await,
                (Some(1), _) => true,
                _ => false,
            }
        })
    }

    /// Whether the window may close now. With unsaved changes, asks first
    /// and closes the window itself once that's resolved.
    pub(super) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // With autosave on, changes are saved rather than asked about.
        if !self.is_dirty() || self.autosave(window, cx) {
            return true;
        }
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if confirmed.await {
                this.update_in(cx, |this, window, _| {
                    // Don't ask again on the way out.
                    if let Some(open) = &mut this.open {
                        open.editor.discard_changes();
                    }
                    window.remove_window();
                })
                .ok();
            }
        })
        .detach();
        false
    }

    pub(super) fn close_window(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // A file that doesn't exist yet (e.g. `kt new.md`) starts as an
        // empty document that saving creates.
        if !path.exists() && path.parent().is_some_and(Path::is_dir) {
            self.load(Document::new_at(path), window, cx);
            self.move_caret(0, false, cx);
            return;
        }
        match Document::open(path) {
            Ok(document) => self.load(document, window, cx),
            Err(err) => show_open_error(&err, window, cx),
        }
    }

    /// Follows a clicked link: Markdown files open in Kertas, as File > Open
    /// does, and anything else opens in its own app. Shows an alert if the
    /// link can't be followed.
    pub fn follow_link(&mut self, destination: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (message, detail) = match links::resolve(destination, self.document_path()) {
            LinkTarget::Markdown(path, anchor) => {
                cx.defer(move |cx| crate::windows::open_path_at(path, anchor, cx));
                return;
            }
            LinkTarget::File(path) => {
                cx.open_with_system(&path);
                return;
            }
            LinkTarget::Url(url) => {
                cx.open_url(&url);
                return;
            }
            LinkTarget::SameDocument(Some(anchor)) => {
                self.go_to_anchor(&anchor, cx);
                return;
            }
            LinkTarget::SameDocument(None) => return,
            LinkTarget::Missing(path) => (
                format!("Couldn't find “{}”", display_name(&path)),
                format!(
                    "The link points to {}, which doesn't exist.",
                    path.display()
                ),
            ),
            LinkTarget::NoFolder(path) => (
                format!("Couldn't follow the link to “{path}”"),
                "The link is relative to the document's folder. Save the document first."
                    .to_owned(),
            ),
        };
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &["OK"], cx);
        cx.spawn(async move |_, _| answer.await.ok()).detach();
    }
}

impl DocumentView {
    /// Scrolls the heading named `name` (as in a `#name` link) to the top
    /// of the window (#91). If there's no such heading, beeps and stays
    /// put. A footnote reference's line goes to the top once it's drawn
    /// (#90).
    pub fn go_to_anchor(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let reference = anchors::find_reference(open.editor.model(), name);
        *open.reference.borrow_mut() = ReferenceTarget {
            offset: reference,
            found: None,
        };
        let Some(index) = anchors::find(open.editor.model(), name) else {
            beep();
            return;
        };
        open.anchor = None;
        open.reveal_caret = false;
        open.list_state.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        cx.notify();
    }
}

/// The system's alert sound, for a link to a heading that doesn't exist.
fn beep() {
    #[cfg(target_os = "macos")]
    objc2_app_kit::NSBeep();
}

/// A path's file name, for alerts.
fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn show_open_error(err: &OpenError, window: &mut Window, cx: &mut Context<DocumentView>) {
    let message = format!("Couldn't open “{}”", err.display_name());
    let detail = capitalize(&err.source.to_string());
    let answer = window.prompt(PromptLevel::Critical, &message, Some(&detail), &["OK"], cx);
    cx.spawn(async move |_, _| answer.await.ok()).detach();
}

fn show_save_error(err: &SaveError, window: &mut Window, cx: &mut Context<DocumentView>) {
    let message = format!("Couldn't save “{}”", err.display_name());
    let detail = capitalize(&err.source.to_string());
    let answer = window.prompt(PromptLevel::Critical, &message, Some(&detail), &["OK"], cx);
    cx.spawn(async move |_, _| answer.await.ok()).detach();
}

pub(super) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
