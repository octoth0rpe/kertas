//! Opening, saving and closing documents.

use std::path::{Path, PathBuf};

use gpui::{Context, ListOffset, PromptLevel, Task, Window, px};
use kertas_core::anchors;
use kertas_core::document::{Document, OpenError, SaveError};
use kertas_core::links::{self, LinkTarget};

use super::{DocumentEvent, DocumentView, Save, SaveAs, ScrollRequest};

impl DocumentView {
    /// Shows `text` view-only, named `name`, replacing the current
    /// document.
    pub fn show_text(
        &mut self,
        text: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_name = Some(name.to_owned());
        self.load(Document::from_text(text), window, cx);
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

    /// Whether the view shows the startup message rather than a document.
    pub fn is_blank(&self) -> bool {
        self.open.is_none()
    }

    /// Whether the view holds nothing worth keeping: no document, or an
    /// untitled one that's still empty.
    pub fn is_pristine(&self) -> bool {
        self.text_name.is_none()
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
                cx.emit(DocumentEvent::TitleChanged);
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

    /// Whether the document can close straight away: it has no unsaved
    /// changes, or autosave just saved them. If not, ask with
    /// [`DocumentView::confirm_close`].
    pub fn ready_to_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // With autosave on, changes are saved rather than asked about.
        !self.is_dirty() || self.autosave(window, cx)
    }

    /// Asks whether to save unsaved changes before the document closes.
    /// Resolves to whether it may close: they were saved, or the user
    /// chose not to save them, and then they aren't asked about again.
    pub fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !confirmed.await {
                return false;
            }
            this.update(cx, |this, _| {
                if let Some(open) = &mut this.open {
                    open.editor.discard_changes();
                }
            })
            .is_ok()
        })
    }

    /// Moves the document's file to `path`, keeping the document open as
    /// it is: its text, unsaved changes, undo history, caret and scroll
    /// position. Watching for changes by other apps follows the file, and
    /// the view sends [`DocumentEvent::TitleChanged`].
    ///
    /// It never overwrites: if something is already at `path`, it fails
    /// and nothing changes (see
    /// [`kertas_core::document::Document::rename`]). Nothing is shown to
    /// the user; that's up to the app.
    pub fn rename_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::io::Result<()> {
        let Some(open) = &mut self.open else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no document is open",
            ));
        };
        if self.text_name.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the document has no file",
            ));
        }
        open.editor.rename(path)?;
        self.watch_file(window, cx);
        cx.emit(DocumentEvent::TitleChanged);
        cx.notify();
        Ok(())
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document. A file
    /// that's empty or doesn't exist yet opens ready for typing.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // A file that doesn't exist yet (e.g. `kt new.md`) starts as an
        // empty document that saving creates.
        if !path.exists() && path.parent().is_some_and(Path::is_dir) {
            self.load(Document::new_at(path), window, cx);
            self.move_caret(0, false, cx);
            return;
        }
        match Document::open(path) {
            Ok(document) => {
                // An empty file is ready for typing too.
                let empty = document.buffer().is_empty();
                self.load(document, window, cx);
                if empty {
                    self.move_caret(0, false, cx);
                }
            }
            Err(err) => show_open_error(&err, window, cx),
        }
    }

    /// Follows a clicked link: Markdown files are passed on with
    /// [`DocumentEvent::OpenFile`], and anything else opens in its own app. Shows an alert if the
    /// link can't be followed.
    pub fn follow_link(&mut self, destination: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (message, detail) = match links::resolve(destination, self.document_path()) {
            LinkTarget::Markdown(path, anchor) => {
                cx.emit(DocumentEvent::OpenFile { path, anchor });
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
        let Some(index) = anchors::find(open.editor.model(), name) else {
            beep();
            return;
        };
        open.scroll_request =
            anchors::find_reference(open.editor.model(), name).map(ScrollRequest::Position);
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
