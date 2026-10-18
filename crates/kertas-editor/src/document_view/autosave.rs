//! Autosave (F6): with autosave on, documents save once
//! typing pauses, when their window or tab loses focus, and when they
//! close.

use std::time::Duration;

use gpui::{Context, PromptLevel, Window};
use kertas_core::document::SaveError;

use super::DocumentView;
use super::files::capitalize;

impl DocumentView {
    /// Saves after the autosave pause, unless there's another edit first.
    pub(super) fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        let Some(seconds) = self.config.autosave else {
            return;
        };
        let handle = self.window_handle;
        self.autosave_epoch += 1;
        let epoch = self.autosave_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds.into()))
                .await;
            handle
                .update(cx, |_, window, cx| {
                    this.update(cx, |view, cx| {
                        if view.autosave_epoch == epoch {
                            view.autosave(window, cx);
                        }
                    })
                })
                .ok();
        })
        .detach();
    }

    /// Called when the view is hidden, such as when another tab is
    /// chosen: autosaves, and ends a mouse drag, whose button may now come
    /// up over another view.
    pub(crate) fn hidden(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_drag();
        self.end_scrollbar_drag(cx);
        self.autosave(window, cx);
    }

    /// Saves the document if autosave is on and it has unsaved changes
    /// that can be saved without asking. Returns whether there's nothing
    /// left unsaved: it was saved, or didn't need to be.
    ///
    /// It isn't saved if it's untitled, while the bar asks about a change
    /// another app made to the file, or after an autosave failed, until
    /// the user saves it. If the file changed since it was last loaded or
    /// saved, the bar asks instead of saving over it.
    pub(crate) fn autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.config.autosave.is_none() || self.confirming {
            return !self.is_dirty();
        }
        let Some(open) = &mut self.open else {
            return true;
        };
        if !open.editor.is_dirty() {
            return true;
        }
        if open.editor.read_only()
            || open.editor.document().path().is_none()
            || open.autosave_paused
            || open.disk_change.is_some()
            // Don't save half-composed text; the composed text is an
            // edit that schedules another save.
            || open.editor.marked().is_some()
        {
            return false;
        }
        if let Some(change) = open.editor.document().disk_change() {
            open.disk_change = Some(change);
            cx.notify();
            return false;
        }
        match open.editor.save() {
            Ok(()) => {
                cx.emit(super::DocumentEvent::TitleChanged);
                cx.notify();
                true
            }
            Err(err) => {
                open.autosave_paused = true;
                show_autosave_error(&err, window, cx);
                false
            }
        }
    }
}

impl DocumentView {
    /// Whether autosave is paused after failing, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn autosave_paused(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.autosave_paused)
    }
}

/// Tells the user an autosave failed, once: autosave then waits for them
/// to save the document themselves.
fn show_autosave_error(err: &SaveError, window: &mut Window, cx: &mut Context<DocumentView>) {
    let message = format!("Couldn't autosave “{}”", err.display_name());
    let detail = format!(
        "{}.\n\nAutosave is paused for this document until you save it with File → Save.",
        capitalize(&err.source.to_string())
    );
    let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &["OK"], cx);
    cx.spawn(async move |_, _| answer.await.ok()).detach();
}
