//! The view that shows a document in a window.
//!
//! Every block is rendered except the one containing the caret, which is
//! revealed as styled Markdown source and edited in place (C4). See
//! `docs/planning/caret-and-reveal.md`.

use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, KeyBinding,
    ListAlignment, ListState, Pixels, Point, PromptLevel, Rems, Subscription,
    SystemWindowTabController, Task, TextLayout, UTF16Selection, Window, actions, div, list, point,
    prelude::*, px, relative, rems, size,
};
use kami::document::{Document, OpenError, SaveError};
use kami::editor::{Change, Editor, Motion, RevealTarget, Selection};
use kami::reveal::{StyledSource, styled_source};
use kami::settings::TextWidth;

use crate::app_settings::{self, AppSettings};
use crate::builtin::Builtin;
use crate::menus::OpenSettings;
use crate::rendered_block::{ClickTarget, render_block};
use crate::revealed_block::{RevealedBlock, TextInput, render_gap, render_revealed};
use crate::tab_strip::{render_tab_strip, select_tab};
use crate::theme::Theme;
use crate::zoom::{self, Zoom};

actions!(
    kami,
    [
        New,
        Open,
        Save,
        SaveAs,
        Print,
        CloseWindow,
        Quit,
        NextTab,
        PreviousTab,
        PageUp,
        PageDown,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        Unreveal,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveWordLeft,
        MoveWordRight,
        MoveLineStart,
        MoveLineEnd,
        MoveDocumentStart,
        MoveDocumentEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        SelectAll,
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        Newline,
        LineBreak,
        IndentTab,
        OutdentTab,
        IndentLines,
        OutdentLines,
        InsertTab,
        MoveBlockUp,
        MoveBlockDown,
        SelectBlockUp,
        SelectBlockDown,
        Cut,
        Copy,
        Paste,
        Undo,
        Redo,
    ]
);

/// Key bindings for the document view (macOS conventions).
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-n", New, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
        KeyBinding::new("cmd-p", Print, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-shift-]", NextTab, None),
        KeyBinding::new("cmd-shift-[", PreviousTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-1", SelectTab1, None),
        KeyBinding::new("cmd-2", SelectTab2, None),
        KeyBinding::new("cmd-3", SelectTab3, None),
        KeyBinding::new("cmd-4", SelectTab4, None),
        KeyBinding::new("cmd-5", SelectTab5, None),
        KeyBinding::new("cmd-6", SelectTab6, None),
        KeyBinding::new("cmd-7", SelectTab7, None),
        KeyBinding::new("cmd-8", SelectTab8, None),
        KeyBinding::new("cmd-9", SelectTab9, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ResetZoom, None),
        KeyBinding::new("escape", Unreveal, None),
        KeyBinding::new("left", MoveLeft, None),
        KeyBinding::new("right", MoveRight, None),
        KeyBinding::new("up", MoveUp, None),
        KeyBinding::new("down", MoveDown, None),
        KeyBinding::new("alt-left", MoveWordLeft, None),
        KeyBinding::new("alt-right", MoveWordRight, None),
        KeyBinding::new("cmd-left", MoveLineStart, None),
        KeyBinding::new("cmd-right", MoveLineEnd, None),
        KeyBinding::new("home", MoveLineStart, None),
        KeyBinding::new("end", MoveLineEnd, None),
        KeyBinding::new("cmd-up", MoveDocumentStart, None),
        KeyBinding::new("pageup", PageUp, None),
        KeyBinding::new("pagedown", PageDown, None),
        KeyBinding::new("cmd-down", MoveDocumentEnd, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("alt-shift-left", SelectWordLeft, None),
        KeyBinding::new("alt-shift-right", SelectWordRight, None),
        KeyBinding::new("cmd-shift-left", SelectLineStart, None),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, None),
        KeyBinding::new("shift-home", SelectLineStart, None),
        KeyBinding::new("shift-end", SelectLineEnd, None),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, None),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("shift-backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("alt-backspace", DeleteWordLeft, None),
        KeyBinding::new("alt-delete", DeleteWordRight, None),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, None),
        KeyBinding::new("enter", Newline, None),
        KeyBinding::new("shift-enter", LineBreak, None),
        KeyBinding::new("tab", IndentTab, None),
        KeyBinding::new("shift-tab", OutdentTab, None),
        KeyBinding::new("cmd-]", IndentLines, None),
        KeyBinding::new("cmd-[", OutdentLines, None),
        KeyBinding::new("alt-tab", InsertTab, None),
        KeyBinding::new("alt-up", MoveBlockUp, None),
        KeyBinding::new("alt-down", MoveBlockDown, None),
        KeyBinding::new("alt-shift-up", SelectBlockUp, None),
        KeyBinding::new("alt-shift-down", SelectBlockDown, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
    ]
}

/// The text column's maximum width for each `text_width` setting (A1), or
/// `None` to use the whole window.
/// Widths are in rems, so zooming keeps the same number of characters per
/// line: 45 rems is 720 px at the default 16 px.
fn column_width(width: TextWidth) -> Option<Rems> {
    match width {
        TextWidth::Standard => Some(rems(45.)),
        TextWidth::Wide => Some(rems(60.)),
        TextWidth::Full => None,
    }
}

/// Code's text size relative to body text (13 px at 16 px).
pub const CODE_TEXT_SIZE: f32 = 0.8125;
pub const CODE_FONT: &str = "Menlo";

/// How long the caret stays on, and then off, while blinking.
const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// An open document: its editor, and the layout state that shows it.
struct OpenDocument {
    editor: Editor,
    list_state: ListState,
    /// The x position to keep when moving up and down.
    goal_x: Option<Pixels>,
    /// The items shown as source, in order: the one holding the caret,
    /// and any others the selection touches.
    revealed: Vec<Revealed>,
    /// The caret's revealed item's text layout from the last frame.
    revealed_layout: Rc<RefCell<Option<TextLayout>>>,
    /// An item to keep at the same place on screen once the next frame is
    /// laid out, and where it was (C7): revealing blocks changes their
    /// heights, which would otherwise move the text under the pointer.
    anchor: Option<(usize, Pixels)>,
    /// Whether to scroll the caret into view once the next frame is laid
    /// out.
    reveal_caret: bool,
}

/// An item shown as source, or with the caret in the gap next to it.
#[derive(Clone)]
struct Revealed {
    target: RevealTarget,
    styled: Rc<StyledSource>,
}

/// Where to put the caret after a neighbor item is revealed and laid out.
#[derive(Clone, Copy)]
enum LineGoal {
    First,
    Last,
}

pub struct DocumentView {
    focus_handle: FocusHandle,
    open: Option<OpenDocument>,
    /// Set when showing one of Kami's built-in documents, which are
    /// view-only.
    builtin: Option<Builtin>,
    theme: Theme,
    text_width: TextWidth,
    /// Whether the blinking caret is currently drawn.
    caret_visible: bool,
    /// Bumped to stop the current blink loop.
    blink_epoch: usize,
    /// Whether the mouse button went down in the document and is still
    /// down, so moving it extends the selection.
    dragging: bool,
    /// Whether a "save changes?" prompt is already showing.
    confirming: bool,
    /// The edited state last shown in the window's close button.
    shown_edited: Option<bool>,
    /// Whether the user closed the bar reporting problems in the settings
    /// file. It shows again when the file changes.
    settings_problems_dismissed: bool,
    _appearance_observer: Subscription,
    _settings_observer: Subscription,
    _zoom_observer: Subscription,
    _tabs_observer: Subscription,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        // Follow the theme setting and the macOS appearance, switching live
        // when either changes.
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = Theme::current(window, cx);
            cx.notify();
        });
        let zoom_observer = cx.observe_global_in::<Zoom>(window, |this, _, cx| {
            this.layout_changed();
            cx.notify();
        });
        let settings_observer = cx.observe_global_in::<AppSettings>(window, |this, window, cx| {
            this.theme = Theme::current(window, cx);
            this.settings_problems_dismissed = false;
            let text_width = app_settings::get(cx).text_width;
            if text_width != this.text_width {
                this.text_width = text_width;
                this.layout_changed();
            }
            cx.notify();
        });
        // Closing the window (e.g. its close button) asks about unsaved
        // changes first.
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });
        Self {
            focus_handle,
            open: None,
            builtin: None,
            theme: Theme::current(window, cx),
            text_width: app_settings::get(cx).text_width,
            caret_visible: true,
            dragging: false,
            blink_epoch: 0,
            confirming: false,
            shown_edited: None,
            settings_problems_dismissed: false,
            _appearance_observer: appearance_observer,
            _settings_observer: settings_observer,
            _zoom_observer: zoom_observer,
            // Redraw the tab strip when tabs are added, removed or renamed.
            _tabs_observer: cx
                .observe_global_in::<SystemWindowTabController>(window, |_, _, cx| cx.notify()),
        }
    }

    /// Shows `document`, replacing the current one.
    fn load(&mut self, document: Document, window: &mut Window, cx: &mut Context<Self>) {
        // Built-in documents are view-only.
        let editor = Editor::new(document, self.builtin.is_some());
        let items = editor.model().items.len();
        self.open = Some(OpenDocument {
            editor,
            list_state: ListState::new(items, ListAlignment::Top, px(1000.)),
            goal_x: None,
            revealed: Vec::new(),
            revealed_layout: Rc::default(),
            anchor: None,
            reveal_caret: false,
        });
        self.stop_blink();
        self.update_window_title(window, cx);
        cx.notify();
    }

    fn update_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let name = match self.builtin {
            Some(builtin) => builtin.title().to_owned(),
            None => open.editor.document().display_name(),
        };
        window.set_window_title(&name);
        window.set_document_path(open.editor.document().path());
        // GPUI doesn't update tab titles from the window title.
        SystemWindowTabController::update_tab_title(
            cx,
            window.window_handle().window_id(),
            name.into(),
        );
    }

    /// Re-measures every block after the column width or text size
    /// changes, keeping the caret's block in view.
    fn layout_changed(&mut self) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.list_state.remeasure();
        *open.revealed_layout.borrow_mut() = None;
        if let Some(selection) = open.editor.selection() {
            let index = open.editor.model().item_at(selection.head);
            open.list_state.scroll_to_reveal_item(index);
        }
    }

    /// How many items the document has, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn item_count(&self) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| open.editor.model().items.len())
    }

    /// Where item `index`'s block starts in the document, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    pub fn item_start(&self, index: usize) -> Option<usize> {
        Some(
            self.open
                .as_ref()?
                .editor
                .model()
                .items
                .get(index)?
                .block
                .start,
        )
    }

    /// Where item `index` starts in the document and its top on screen,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    pub fn item_position(&self, index: usize) -> Option<(usize, Pixels)> {
        let open = self.open.as_ref()?;
        let item = open.editor.model().items.get(index)?;
        let top = open.list_state.bounds_for_item(index)?.top();
        Some((item.block.start, top))
    }

    /// How many items are revealed, and the selection, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn reveal_state(&self) -> (usize, Option<Selection>) {
        self.open.as_ref().map_or((0, None), |open| {
            (open.revealed.len(), open.editor.selection())
        })
    }

    /// The list's scroll position, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn scroll_top(&self) -> Option<(usize, Pixels)> {
        let top = self.open.as_ref()?.list_state.logical_scroll_top();
        Some((top.item_ix, top.offset_in_item))
    }

    /// The document's text, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn text(&self) -> String {
        self.open
            .as_ref()
            .map(|open| open.editor.text().to_owned())
            .unwrap_or_default()
    }

    /// Whether there's a caret, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn has_caret(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.editor.selection().is_some())
    }

    /// The theme the view is drawn with, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_dirty(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.editor.is_dirty())
    }

    // Files

    /// File > Print…: the standard print panel, as a sheet on this window.
    fn print(&mut self, _: &Print, window: &mut Window, _: &mut Context<Self>) {
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
            crate::print::print(open.editor.model(), &title, &ns_window, mtm);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
    }

    /// The document's model, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn model(&self) -> Option<Rc<kami::model::Model>> {
        self.open.as_ref().map(|open| open.editor.model().clone())
    }

    /// Shows one of Kami's built-in documents, view-only.
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

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.save_document(false, window, cx).detach();
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
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
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.is_dirty() {
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

    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // A file that doesn't exist yet (e.g. `kami new.md`) starts as an
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

    /// Places the caret at document `offset` after a click.
    pub fn click(
        &mut self,
        offset: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        if let Some(open) = &mut self.open {
            open.goal_x = None;
            open.editor.unmark();
        }
        self.dragging = true;
        self.move_caret(offset, extend, cx);
    }

    /// Stops extending the selection when the mouse moves.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Extends the selection to document `offset` while dragging.
    pub fn drag_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let unchanged = self
            .open
            .as_ref()
            .and_then(|open| open.editor.selection())
            .is_some_and(|selection| selection.head == offset);
        if self.dragging && !unchanged {
            self.set_caret(offset, true, cx);
        }
    }

    /// Page Up and Page Down: scroll by a page, leaving the caret where it
    /// is, as on macOS.
    fn scroll_page(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        // Keep a little of the last page in view for context.
        let page = (open.list_state.viewport_bounds().size.height - px(48.)).max(px(48.));
        open.list_state.scroll_by(if down { page } else { -page });
        cx.notify();
    }

    /// Once a frame is laid out: keeps the anchored item in place, then
    /// scrolls the caret into view if it moved off screen.
    fn after_layout(&mut self, cx: &mut Context<Self>) {
        let mut shift = px(0.);
        if let Some(open) = &mut self.open
            && let Some((index, top)) = open.anchor.take()
            && let Some(bounds) = open.list_state.bounds_for_item(index)
        {
            shift = bounds.top() - top;
            open.list_state.scroll_by(shift);
        }
        let reveal_caret = self
            .open
            .as_mut()
            .is_some_and(|open| std::mem::take(&mut open.reveal_caret));
        if reveal_caret
            && let Some((layout, start)) = self.revealed_layout()
            && let Some(open) = &self.open
            && let Some(selection) = open.editor.selection()
            && let Some(position) = layout.position_for_index(selection.head.saturating_sub(start))
        {
            let viewport = open.list_state.viewport_bounds();
            let line = layout.line_height();
            let top = position.y - shift;
            let bottom = top + line;
            if top < viewport.top() {
                open.list_state.scroll_by(top - viewport.top() - line);
            } else if bottom > viewport.bottom() {
                open.list_state.scroll_by(bottom - viewport.bottom() + line);
            }
        }
        cx.notify();
    }

    /// Moves the caret to `offset` as a navigation (not an edit): it ends
    /// the current undo group.
    fn move_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        if let Some(open) = &mut self.open {
            open.editor.break_group();
        }
        self.set_caret(offset, extend, cx);
    }

    /// Moves the caret to `offset`, revealing its item.
    fn set_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let selection = open.editor.selection_to(offset, extend);
        self.set_selection(selection, cx);
    }

    fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        // Read-only documents have no caret, so every movement, click and
        // edit stops here.
        if !open.editor.select(selection) {
            return;
        }
        let index = open.editor.model().item_at(selection.head);
        // Where the caret's item is now, to keep it there as blocks reveal.
        let top = open
            .list_state
            .bounds_for_item(index)
            .map(|bounds| bounds.top());
        if reveal(open) {
            open.anchor = top.map(|top| (index, top));
        }
        if top.is_none() {
            open.list_state.scroll_to_reveal_item(index);
        }
        open.reveal_caret = true;
        self.restart_blink(cx);
        cx.notify();
    }

    /// Shows the caret and starts blinking it again, so it stays solid
    /// while the user is moving it.
    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        self.caret_visible = true;
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CARET_BLINK_INTERVAL).await;
                let blinking = this.update(cx, |this, cx| {
                    if this.blink_epoch != epoch {
                        return false;
                    }
                    this.caret_visible = !this.caret_visible;
                    cx.notify();
                    true
                });
                if !matches!(blinking, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    fn stop_blink(&mut self) {
        self.blink_epoch += 1;
    }

    fn unreveal(&mut self, _: &Unreveal, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_editing(cx);
    }

    /// Hides the caret and shows every block rendered.
    pub fn finish_editing(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.editor.deselect();
        open.goal_x = None;
        reveal(open);
        open.anchor = None;
        self.stop_blink();
        cx.notify();
    }

    /// The caret's offset, placing it at the document start if there's no
    /// caret yet.
    fn caret(&self) -> Option<usize> {
        Some(self.open.as_ref()?.editor.caret())
    }

    /// Applies a layout-independent movement. `forward` is the direction,
    /// also used to collapse a selection to the matching edge.
    fn move_by(&mut self, motion: Motion, forward: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let target = open.editor.motion_target(motion, forward, extend);
        open.goal_x = None;
        self.move_caret(target, extend, cx);
    }

    /// The revealed item's layout and start offset, if it's been painted.
    fn revealed_layout(&self) -> Option<(TextLayout, usize)> {
        let open = self.open.as_ref()?;
        open.editor.selection()?;
        let revealed = open.revealed.iter().find(|revealed| revealed.target.head)?;
        let layout = open.revealed_layout.borrow().clone()?;
        Some((layout, revealed.target.range.start))
    }

    /// Moves the caret up or down a visual line, into the neighbor item at
    /// the revealed item's edge.
    fn move_vertically(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(caret) = self.caret() else {
            return;
        };
        let Some((layout, item_start)) = self.revealed_layout() else {
            self.move_caret(caret, extend, cx);
            return;
        };
        let Some(position) = layout.position_for_index(caret - item_start) else {
            return;
        };
        let Some(open) = &mut self.open else {
            return;
        };
        let line_height = layout.line_height();
        let goal_x = *open.goal_x.get_or_insert(position.x);
        let target_y = if down {
            position.y + line_height * 1.5
        } else {
            position.y - line_height * 0.5
        };
        let bounds = layout.bounds();

        if target_y >= bounds.top() && target_y < bounds.bottom() {
            let (Ok(index) | Err(index)) = layout.index_for_position(point(goal_x, target_y));
            self.move_caret(item_start + index, extend, cx);
            return;
        }

        // Past the edge of what's revealed: reveal the neighbor, then place
        // the caret on its first or last line once it's laid out.
        let Some(neighbor) = open.editor.vertical_neighbor(down) else {
            let edge = if down { open.editor.text().len() } else { 0 };
            self.move_caret(edge, extend, cx);
            return;
        };
        self.move_caret(neighbor, extend, cx);
        let goal = if down {
            LineGoal::First
        } else {
            LineGoal::Last
        };
        let view = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            view.update(cx, |this, cx| {
                this.resolve_line_goal(goal, goal_x, extend, cx)
            })
            .ok();
        });
    }

    /// Places the caret at `goal_x` on the revealed item's first or last
    /// line, after it has been laid out.
    fn resolve_line_goal(
        &mut self,
        goal: LineGoal,
        goal_x: Pixels,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((layout, item_start)) = self.revealed_layout() else {
            return;
        };
        let bounds = layout.bounds();
        let half_line = layout.line_height() * 0.5;
        let y = match goal {
            LineGoal::First => bounds.top() + half_line,
            LineGoal::Last => bounds.bottom() - half_line,
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(goal_x, y));
        self.move_caret(item_start + index, extend, cx);
        if let Some(open) = &mut self.open {
            open.goal_x = Some(goal_x);
        }
    }

    /// The start or end of the caret's visual line.
    fn line_edge(&self, end: bool) -> Option<usize> {
        let caret = self.caret()?;
        let (layout, item_start) = self.revealed_layout()?;
        let position = layout.position_for_index(caret - item_start)?;
        let bounds = layout.bounds();
        let y = position.y + layout.line_height() * 0.5;
        let x = if end {
            bounds.right() + px(10_000.)
        } else {
            bounds.left() - px(10_000.)
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(x, y));
        Some(item_start + index)
    }

    fn move_to_line_edge(&mut self, end: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(target) = self.line_edge(end).or(self.caret()) else {
            return;
        };
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.move_caret(target, extend, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.editor.break_group();
        let selection = open.editor.all();
        self.set_selection(selection, cx);
    }

    /// Moves the caret to the start of the next or previous block's text.
    /// With no caret, starts at the first block on screen.
    fn move_to_block(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let first_visible = open.list_state.logical_scroll_top().item_ix;
        let Some(offset) = open.editor.block_target(down, first_visible) else {
            return;
        };
        open.goal_x = None;
        self.move_caret(offset, extend, cx);
    }

    // Editing

    /// Makes an edit with the editor and shows the result. The caret's
    /// goal x position is reset.
    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Editor) -> Option<Change>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Some(change) = edit(&mut open.editor) {
            open.goal_x = None;
            self.apply(change, cx);
        }
    }

    /// Re-measures the items an edit changed and shows its selection,
    /// which reveals its item again.
    fn apply(&mut self, change: Change, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.list_state
            .splice(change.splice.old, change.splice.new_count);
        // The revealed items' indices and sources may have changed.
        open.revealed.clear();
        *open.revealed_layout.borrow_mut() = None;
        self.set_selection(change.selection, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(line_start) = self.line_edge(false) else {
            return;
        };
        self.edit(cx, |editor| editor.delete_to_line_start(line_start));
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self
            .open
            .as_ref()
            .and_then(|open| open.editor.selected_text())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.edit(cx, Editor::cut);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.edit(cx, |editor| editor.paste(&text));
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(change) = self.open.as_mut().and_then(|open| open.editor.undo()) {
            self.apply(change, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(change) = self.open.as_mut().and_then(|open| open.editor.redo()) {
            self.apply(change, cx);
        }
    }

    // UTF-16 conversion for the platform input handler.

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| open.editor.to_utf16(offset))
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.open
            .as_ref()
            .map_or(0..0, |open| open.editor.range_from_utf16(range))
    }
}

/// Reveals the items the selection touches (or none), re-measuring the
/// items that change. Returns whether anything changed.
fn reveal(open: &mut OpenDocument) -> bool {
    let targets = open.editor.reveal_targets();
    if open
        .revealed
        .iter()
        .map(|revealed| &revealed.target)
        .eq(&targets)
    {
        return false;
    }

    // Re-measure every item that's revealed now or was before.
    let mut changed: Vec<usize> = open
        .revealed
        .iter()
        .map(|revealed| &revealed.target)
        .chain(&targets)
        .map(|target| target.index)
        .collect();
    changed.sort_unstable();
    changed.dedup();
    for index in changed {
        open.list_state.splice(index..index + 1, 1);
    }
    *open.revealed_layout.borrow_mut() = None;
    let model = open.editor.model();
    open.revealed = targets
        .into_iter()
        .map(|target| {
            let source = &model.text[target.range.clone()];
            let styled = styled_source(source, target.kind, &model.definitions);
            Revealed {
                target,
                styled: Rc::new(styled),
            }
        })
        .collect();
    true
}

impl EntityInputHandler for DocumentView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let editor = &self.open.as_ref()?.editor;
        let range = editor.range_from_utf16(&range_utf16);
        actual_range.replace(editor.range_to_utf16(&range));
        Some(editor.text()[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let editor = &self.open.as_ref()?.editor;
        let selection = editor.selection()?;
        Some(UTF16Selection {
            range: editor.range_to_utf16(&selection.range()),
            reversed: selection.head < selection.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let editor = &self.open.as_ref()?.editor;
        Some(editor.range_to_utf16(&editor.marked()?))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = &mut self.open {
            open.editor.unmark();
        }
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |editor| editor.replace_text_in_range(range_utf16, text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |editor| {
            editor.replace_and_mark_text_in_range(range_utf16, text, new_selected_range_utf16)
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (layout, item_start) = self.revealed_layout()?;
        let start = layout.position_for_index(range.start.checked_sub(item_start)?)?;
        let end = layout
            .position_for_index(range.end.checked_sub(item_start)?)
            .unwrap_or(start);
        let line_height = layout.line_height();
        let width = if end.y == start.y {
            end.x - start.x
        } else {
            px(0.)
        };
        Some(Bounds::new(start, size(width, line_height)))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let (layout, item_start) = self.revealed_layout()?;
        let (Ok(index) | Err(index)) = layout.index_for_position(point);
        Some(self.offset_to_utf16(item_start + index))
    }
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

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl DocumentView {
    /// A bar at the bottom of the window reporting problems in the settings
    /// file (G4), until the user closes it.
    fn render_settings_problems(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.settings_problems_dismissed {
            return None;
        }
        let problems = &cx.try_global::<AppSettings>()?.problems;
        let first = problems.first()?;
        let mut message = format!("Settings file, {first}");
        if problems.len() > 1 {
            message.push_str(&format!(" (and {} more)", problems.len() - 1));
        }
        let theme = self.theme;
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py(px(6.))
                .border_t_1()
                .border_color(theme.rule)
                .bg(theme.table_header_background)
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(theme.muted_text)
                .child(div().flex_1().child(message))
                .child(
                    div()
                        .px_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|this| this.bg(theme.rule))
                        .child("×")
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.settings_problems_dismissed = true;
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Document text is sized in rems, so this sets the zoom.
        window.set_rem_size(px(zoom::size(cx) as f32));
        if let Some(open) = &self.open
            && (open.anchor.is_some() || open.reveal_caret)
        {
            let view = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                view.update(cx, |this, cx| this.after_layout(cx)).ok();
            });
        }
        // Show unsaved changes in the window's close button.
        let edited = self.is_dirty();
        if self.shown_edited != Some(edited) {
            window.set_window_edited(edited);
            self.shown_edited = Some(edited);
        }
        let tab_strip = render_tab_strip(window, cx, self.theme, edited);
        let problems_bar = self.render_settings_problems(cx);

        let content = match &self.open {
            Some(open) => {
                let model = open.editor.model().clone();
                let selection = open.editor.selection();
                let marked = open.editor.marked();
                let revealed = open.revealed.clone();
                let layout_slot = open.revealed_layout.clone();
                let caret_visible = self.caret_visible;
                let column_width = column_width(self.text_width);
                let base_font = window.text_style().font();
                let theme = self.theme;
                let view = cx.entity();
                let focus_handle = self.focus_handle.clone();
                list(open.list_state.clone(), move |index, _, _| {
                    let item = &model.items[index];
                    match revealed
                        .iter()
                        .find(|revealed| revealed.target.index == index)
                    {
                        Some(Revealed {
                            target:
                                RevealTarget {
                                    range, gap, head, ..
                                },
                            styled,
                        }) => {
                            let range = range.clone();
                            let selection = selection.unwrap_or_default();
                            let local =
                                |offset: usize| offset.clamp(range.start, range.end) - range.start;
                            let selected = selection.range();
                            let caret = (caret_visible
                                && *head
                                && (range.contains(&selection.head)
                                    || selection.head == range.end))
                                .then(|| selection.head - range.start);
                            let marked = marked
                                .as_ref()
                                .map(|marked| local(marked.start)..local(marked.end));
                            let block = RevealedBlock {
                                source: &model.text[range.clone()],
                                styled,
                                selection: local(selected.start)..local(selected.end),
                                caret,
                                marked,
                                first: index == 0,
                            };
                            let input = head.then(|| TextInput {
                                focus_handle: focus_handle.clone(),
                                view: view.clone(),
                                layout_slot: layout_slot.clone(),
                            });
                            match gap {
                                Some(gap) if let Some(input) = input => render_gap(
                                    &item.rendered,
                                    *gap,
                                    block,
                                    &base_font,
                                    theme,
                                    // Clicks on the block place the caret in it.
                                    ClickTarget {
                                        view: view.downgrade(),
                                        block_start: item.block.start,
                                        column_width,
                                    },
                                    input,
                                ),
                                _ => render_revealed(
                                    block,
                                    &base_font,
                                    theme,
                                    ClickTarget {
                                        view: view.downgrade(),
                                        block_start: range.start,
                                        column_width,
                                    },
                                    input,
                                ),
                            }
                        }
                        _ => render_block(
                            &item.rendered,
                            index == 0,
                            &base_font,
                            theme,
                            ClickTarget {
                                view: view.downgrade(),
                                block_start: item.block.start,
                                column_width,
                            },
                        ),
                    }
                })
                .size_full()
                .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(self.theme.muted_text)
                .child("Open a Markdown file with ⌘O, or start a new one with ⌘N")
                .into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::print))
            .on_action(cx.listener(Self::close_window))
            .on_action(|_: &NextTab, window, cx| {
                SystemWindowTabController::select_next_tab(cx, window.window_handle().window_id())
            })
            .on_action(|_: &PreviousTab, window, cx| {
                SystemWindowTabController::select_previous_tab(
                    cx,
                    window.window_handle().window_id(),
                )
            })
            .on_action(|_: &SelectTab1, window, cx| select_tab(0, window, cx))
            .on_action(|_: &SelectTab2, window, cx| select_tab(1, window, cx))
            .on_action(|_: &SelectTab3, window, cx| select_tab(2, window, cx))
            .on_action(|_: &SelectTab4, window, cx| select_tab(3, window, cx))
            .on_action(|_: &SelectTab5, window, cx| select_tab(4, window, cx))
            .on_action(|_: &SelectTab6, window, cx| select_tab(5, window, cx))
            .on_action(|_: &SelectTab7, window, cx| select_tab(6, window, cx))
            .on_action(|_: &SelectTab8, window, cx| select_tab(7, window, cx))
            .on_action(|_: &SelectTab9, window, cx| select_tab(8, window, cx))
            // Only while possible, so the menu items are disabled at the
            // limits.
            .when_some(zoom::larger(cx), |this, size| {
                this.on_action(move |_: &ZoomIn, _, cx| zoom::set(size, cx))
            })
            .when_some(zoom::smaller(cx), |this, size| {
                this.on_action(move |_: &ZoomOut, _, cx| zoom::set(size, cx))
            })
            .when(zoom::is_zoomed(cx), |this| {
                this.on_action(|_: &ResetZoom, _, cx| zoom::reset(cx))
            })
            .on_action(cx.listener(Self::unreveal))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.scroll_page(false, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.scroll_page(true, cx)))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.end_drag()),
            )
            .on_action(cx.listener(|this, _: &MoveLeft, _, cx| {
                this.move_by(Motion::Grapheme, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| {
                this.move_by(Motion::Grapheme, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.move_by(Motion::Grapheme, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_by(Motion::Grapheme, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordLeft, _, cx| {
                this.move_by(Motion::Word, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordRight, _, cx| {
                this.move_by(Motion::Word, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.move_by(Motion::Word, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.move_by(Motion::Word, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentStart, _, cx| {
                this.move_by(Motion::Document, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentEnd, _, cx| {
                this.move_by(Motion::Document, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentStart, _, cx| {
                this.move_by(Motion::Document, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentEnd, _, cx| {
                this.move_by(Motion::Document, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.move_vertically(false, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.move_vertically(true, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectUp, window, cx| {
                this.move_vertically(false, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDown, window, cx| {
                this.move_vertically(true, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveLineStart, _, cx| {
                this.move_to_line_edge(false, false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &MoveLineEnd, _, cx| this.move_to_line_edge(true, false, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectLineStart, _, cx| {
                this.move_to_line_edge(false, true, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SelectLineEnd, _, cx| {
                    this.move_to_line_edge(true, true, cx)
                }),
            )
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Grapheme, false))
            }))
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Grapheme, true))
            }))
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Word, false))
            }))
            .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Word, true))
            }))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(
                cx.listener(|this, _: &Newline, _, cx| {
                    this.edit(cx, |editor| editor.newline(false))
                }),
            )
            .on_action(cx.listener(|this, _: &LineBreak, _, cx| {
                this.edit(cx, |editor| editor.newline(true))
            }))
            .on_action(cx.listener(|this, _: &IndentTab, _, cx| {
                this.edit(cx, |editor| editor.indent(false, false))
            }))
            .on_action(cx.listener(|this, _: &OutdentTab, _, cx| {
                this.edit(cx, |editor| editor.indent(true, false))
            }))
            .on_action(cx.listener(|this, _: &IndentLines, _, cx| {
                this.edit(cx, |editor| editor.indent(false, true))
            }))
            .on_action(cx.listener(|this, _: &OutdentLines, _, cx| {
                this.edit(cx, |editor| editor.indent(true, true))
            }))
            .on_action(
                cx.listener(|this, _: &InsertTab, _, cx| {
                    this.edit(cx, |editor| editor.insert("\t"))
                }),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockUp, _, cx| this.move_to_block(false, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockDown, _, cx| this.move_to_block(true, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectBlockUp, _, cx| this.move_to_block(false, true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectBlockDown, _, cx| this.move_to_block(true, true, cx)),
            )
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .size_full()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(rems(1.))
            .line_height(relative(1.6))
            .flex()
            .flex_col()
            .children(tab_strip)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    // Clicks below the last block finish editing.
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.finish_editing(cx)),
                    )
                    .child(content),
            )
            .children(problems_bar)
    }
}
