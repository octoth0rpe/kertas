//! Noticing when another app changes the document's file (F5): reloading
//! it, or asking first if there are unsaved changes.

use std::path::Path;
use std::time::Duration;

use gpui::{AnyElement, Context, Window, div, prelude::*, px};
use kertas_core::document::{DiskChange, DiskChangeKind};

use super::{DocumentView, Save, SaveAs, caret::reveal};
use crate::file_watch;

/// How long to wait after the file changes before reading it, so a save
/// made in several steps (write a new file, rename it over the old one)
/// has finished.
const SETTLE: Duration = Duration::from_millis(100);

impl DocumentView {
    /// Starts watching the document's file, if it has one and it isn't
    /// already watched. Text shown view-only and untitled documents aren't
    /// watched.
    pub(super) fn watch_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_name.is_some() {
            return;
        }
        let Some(open) = &mut self.open else {
            return;
        };
        let path = open.editor.document().path().map(Path::to_owned);
        if open.watched.as_ref().map(|(path, _)| path) == path.as_ref() {
            return;
        }
        open.watched = path.map(|path| {
            let mut watch = file_watch::watch(path.clone());
            let task = cx.spawn_in(window, async move |this, cx| {
                while watch.changed().await {
                    cx.background_executor().timer(SETTLE).await;
                    watch.take_pending();
                    if this
                        .update_in(cx, |this, window, cx| this.check_disk(window, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });
            (path, task)
        });
    }

    /// Looks at the file after it may have changed: reloads it if there
    /// are no unsaved changes, otherwise asks with the bar at the top.
    pub(super) fn check_disk(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let change = open.editor.document().disk_change();
        match change {
            None => {
                // Changed back, say, or saved by Kertas.
                open.disk_change = None;
            }
            Some(change)
                if matches!(change.kind, DiskChangeKind::Changed(_)) && !open.editor.is_dirty() =>
            {
                open.disk_change = None;
                self.reload(&change, cx);
            }
            Some(change) => open.disk_change = Some(change),
        }
        cx.notify();
    }

    /// Replaces the text with the file's, as one undo step, keeping the
    /// scroll position and, where it can, the caret.
    fn reload(&mut self, change: &DiskChange, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(reload) = open.editor.reload(change) else {
            return;
        };
        let changed = reload.splice.is_some();
        if let Some(splice) = reload.splice {
            open.splice(&splice);
        }
        // The revealed items' indices and sources may have changed.
        open.revealed.clear();
        *open.revealed_layout.borrow_mut() = None;
        open.goal_x = None;
        reveal(open);
        if changed {
            self.schedule_edited(cx);
        }
        cx.notify();
    }

    /// The bar's Reload button: replaces unsaved changes with the file.
    pub fn reload_pending(&mut self, cx: &mut Context<Self>) {
        let Some(change) = self.open.as_mut().and_then(|open| open.disk_change.take()) else {
            return;
        };
        self.reload(&change, cx);
    }

    /// The bar's Keep Mine button: keeps the text as it is, so the next
    /// save overwrites the file.
    pub fn keep_mine(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Some(change) = open.disk_change.take() {
            open.editor.accept_disk_change(&change);
        }
        self.schedule_autosave(cx);
        cx.notify();
    }

    /// Whether the bar at the top is asking about a change to the file,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn disk_change_shown(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.disk_change.is_some())
    }

    /// A bar at the top of the document when another app changed, deleted
    /// or moved its file and Kertas didn't simply reload it.
    pub(super) fn render_disk_change(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        let change = open.disk_change.as_ref()?;
        let name = open.editor.document().display_name();
        let (message, buttons): (String, &[(&str, BarAction)]) = match &change.kind {
            DiskChangeKind::Changed(_) => (
                format!(
                    "“{name}” was changed by another app. Reload discards your changes; \
                     Keep Mine keeps them, and saving writes over the file."
                ),
                &[
                    ("Reload", BarAction::Reload),
                    ("Keep Mine", BarAction::KeepMine),
                ],
            ),
            DiskChangeKind::Missing => (
                format!(
                    "“{name}” was deleted or moved. Save to put it back, or Save As to \
                     put it somewhere else."
                ),
                &[
                    ("Save", BarAction::Save),
                    ("Save As…", BarAction::SaveAs),
                    ("Dismiss", BarAction::KeepMine),
                ],
            ),
            DiskChangeKind::Unreadable(err) => (
                format!(
                    "“{name}” was changed by another app, but can't be read: {err}. \
                     Saving will write over it."
                ),
                &[("Keep Mine", BarAction::KeepMine)],
            ),
        };
        let theme = self.theme;
        let buttons = buttons.iter().map(|&(label, action)| {
            super::render::bar_button(theme, label, label, label).on_click(cx.listener(
                move |this, _, window, cx| {
                    // The bar closes, so focus would be left on nothing.
                    this.focus_handle.focus(window, cx);
                    match action {
                        BarAction::Reload => this.reload_pending(cx),
                        BarAction::KeepMine => this.keep_mine(cx),
                        BarAction::Save => window.dispatch_action(Box::new(Save), cx),
                        BarAction::SaveAs => window.dispatch_action(Box::new(SaveAs), cx),
                    }
                },
            ))
        });
        Some(
            div()
                .id("disk-change")
                .role(gpui::Role::Alert)
                .aria_label(message.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py(px(6.))
                .border_b_1()
                .border_color(theme.rule)
                .bg(theme.table_header_background)
                .text_size(px(12.))
                .line_height(px(16.))
                .child(div().flex_1().child(message))
                .children(buttons)
                .into_any_element(),
        )
    }
}

#[derive(Clone, Copy)]
enum BarAction {
    Reload,
    KeepMine,
    Save,
    SaveAs,
}
