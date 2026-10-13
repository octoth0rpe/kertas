//! The view that shows a document in a window.
//!
//! Every block is rendered except the one containing the caret, which is
//! revealed as styled Markdown source and edited in place (C4). See
//! `docs/planning/caret-and-reveal.md`.

use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, KeyBinding,
    ListAlignment, ListState, Pixels, Point, PromptLevel, Rems, Subscription,
    SystemWindowTabController, Task, TextLayout, UTF16Selection, Window, actions, div, list, point,
    prelude::*, px, relative, rems, size,
};
use kami::blocks::BlockKind;
use kami::document::{Document, OpenError, SaveError};
use kami::editing;
use kami::history::{EditKind, History};
use kami::model::{Gap, Model};
use kami::movement::{self, Selection};
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

/// An open document and everything needed to display and edit it.
struct OpenDocument {
    document: Document,
    model: Rc<Model>,
    history: History,
    list_state: ListState,
    /// The selection, or `None` before the first click or key press.
    selection: Option<Selection>,
    /// Text being composed with an input method.
    marked: Option<Range<usize>>,
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
    index: usize,
    /// The source shown: the item's source, or an empty range at the caret
    /// in a gap.
    range: Range<usize>,
    gap: Option<Gap>,
    /// Whether this item holds the caret. It takes typing and shows the
    /// hint; other revealed items are only part of the selection.
    head: bool,
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
        let model = Model::new(&document.buffer().rope().to_string());
        let list_state = ListState::new(model.items.len(), ListAlignment::Top, px(1000.));
        self.open = Some(OpenDocument {
            document,
            model: Rc::new(model),
            history: History::default(),
            list_state,
            selection: None,
            marked: None,
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
            None => open.document.display_name(),
        };
        window.set_window_title(&name);
        window.set_document_path(open.document.path());
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
        if let Some(selection) = open.selection {
            let index = open.model.item_at(selection.head);
            open.list_state.scroll_to_reveal_item(index);
        }
    }

    /// How many items the document has, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn item_count(&self) -> usize {
        self.open.as_ref().map_or(0, |open| open.model.items.len())
    }

    /// Where item `index`'s block starts in the document, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    pub fn item_start(&self, index: usize) -> Option<usize> {
        Some(self.open.as_ref()?.model.items.get(index)?.block.start)
    }

    /// Where item `index` starts in the document and its top on screen,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    pub fn item_position(&self, index: usize) -> Option<(usize, Pixels)> {
        let open = self.open.as_ref()?;
        let item = open.model.items.get(index)?;
        let top = open.list_state.bounds_for_item(index)?.top();
        Some((item.block.start, top))
    }

    /// How many items are revealed, and the selection, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn reveal_state(&self) -> (usize, Option<Selection>) {
        self.open
            .as_ref()
            .map_or((0, None), |open| (open.revealed.len(), open.selection))
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
            .map(|open| open.model.text.to_string())
            .unwrap_or_default()
    }

    /// Whether there's a caret, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn has_caret(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.selection.is_some())
    }

    /// The theme the view is drawn with, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_dirty(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.document.is_dirty())
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
                None => open.document.display_name(),
            };
            crate::print::print(&open.model, &title, &ns_window, mtm);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
    }

    /// The document's model, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn model(&self) -> Option<Rc<Model>> {
        self.open.as_ref().map(|open| open.model.clone())
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
        self.open.as_ref()?.document.path()
    }

    /// Whether the window holds nothing worth keeping: no document, or an
    /// untitled one that's still empty. Opening a file reuses such a window
    /// instead of adding a tab.
    pub fn is_pristine(&self) -> bool {
        self.builtin.is_none()
            && self.open.as_ref().is_none_or(|open| {
                open.document.path().is_none() && open.document.buffer().is_empty()
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
        // Built-in documents have nothing to save.
        if self.builtin.is_some() {
            return Task::ready(false);
        }
        let current_path = open.document.path().map(Path::to_owned);
        if !choose_path && current_path.is_some() {
            let result = open.document.save();
            return Task::ready(self.finish_save(result, window, cx));
        }

        let directory = current_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let suggested = match &current_path {
            Some(_) => open.document.display_name(),
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
                let result = open.document.save_as(path);
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
            .map(|open| open.document.display_name())
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
                        open.document = Document::untitled();
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
            open.marked = None;
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
            .and_then(|open| open.selection)
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
            && let Some(selection) = open.selection
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
            open.history.break_group();
        }
        self.set_caret(offset, extend, cx);
    }

    /// Moves the caret to `offset`, revealing its item.
    fn set_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let mut selection = open.selection.unwrap_or(Selection::caret(offset));
        selection.move_to(offset, extend);
        self.set_selection(selection, cx);
    }

    fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        // Built-in documents are view-only: no caret, so every movement,
        // click and edit stops here.
        if self.builtin.is_some() {
            return;
        }
        let Some(open) = &mut self.open else {
            return;
        };
        open.selection = Some(selection);
        let index = open.model.item_at(selection.head);
        // Where the caret's item is now, to keep it there as blocks reveal.
        let top = open
            .list_state
            .bounds_for_item(index)
            .map(|bounds| bounds.top());
        if reveal(open, Some(selection)) {
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
        open.selection = None;
        open.goal_x = None;
        open.marked = None;
        open.history.break_group();
        reveal(open, None);
        open.anchor = None;
        self.stop_blink();
        cx.notify();
    }

    /// The caret's offset, placing it at the document start if there's no
    /// caret yet.
    fn caret(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        Some(open.selection.map_or(0, |selection| selection.head))
    }

    /// Applies a layout-independent movement. `forward` is the direction,
    /// used to collapse a selection to the matching edge (as on macOS).
    fn move_by(
        &mut self,
        extend: bool,
        forward: bool,
        cx: &mut Context<Self>,
        motion: impl FnOnce(&str, usize) -> usize,
    ) {
        let (Some(open), Some(caret)) = (&self.open, self.caret()) else {
            return;
        };
        let target = match open.selection {
            // The first key press just places the caret.
            None => caret,
            Some(selection) if !extend && !selection.is_empty() => {
                let range = selection.range();
                if forward { range.end } else { range.start }
            }
            Some(_) => motion(&open.model.text, caret),
        };
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.move_caret(target, extend, cx);
    }

    /// The revealed item's layout and start offset, if it's been painted.
    fn revealed_layout(&self) -> Option<(TextLayout, usize)> {
        let open = self.open.as_ref()?;
        open.selection?;
        let revealed = open.revealed.iter().find(|revealed| revealed.head)?;
        let layout = open.revealed_layout.borrow().clone()?;
        Some((layout, revealed.range.start))
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
        // the caret on its first or last line once it's laid out. The
        // neighbor may be the gap next to the block, or the block next to
        // the gap.
        let index = open.model.item_at(caret);
        let item = &open.model.items[index];
        let source = open.model.source_range(index);
        let gap = open
            .revealed
            .iter()
            .find(|revealed| revealed.head)
            .and_then(|revealed| revealed.gap);
        let neighbor = match (gap, down) {
            (Some(Gap::Above), true) => Some(source.start),
            (Some(Gap::Below), false) => Some(source.end),
            (None, true) if source.end < item.range.end => {
                Some(movement::next_grapheme(&open.model.text, source.end))
            }
            (None, false) if source.start > item.range.start => {
                Some(movement::prev_grapheme(&open.model.text, source.start))
            }
            (_, true) => open.model.items.get(index + 1).map(|item| item.range.start),
            (_, false) => index
                .checked_sub(1)
                .map(|index| open.model.items[index].range.end),
        };
        let Some(neighbor) = neighbor else {
            let edge = if down { open.model.text.len() } else { 0 };
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
        open.history.break_group();
        let selection = Selection {
            anchor: 0,
            head: open.model.text.len(),
        };
        self.set_selection(selection, cx);
    }

    // Editing

    /// Replaces `range` with `text`, records it for undo, and updates the
    /// display. The caret ends after the inserted text.
    fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind, cx: &mut Context<Self>) {
        if self.builtin.is_some() {
            return;
        }
        let Some(open) = &mut self.open else {
            return;
        };
        let before = open.selection.unwrap_or(Selection::caret(range.start));
        let selection = open.history.replace(
            open.document.buffer_mut(),
            range,
            text,
            kind,
            before,
            Instant::now(),
        );
        open.marked = None;
        open.goal_x = None;
        text_changed(open);
        self.set_selection(selection, cx);
    }

    /// The selected range, or `None` without a caret.
    fn selected_range(&self) -> Option<Range<usize>> {
        Some(self.open.as_ref()?.selection?.range())
    }

    /// Deletes the selection, or else the range from the caret to
    /// `target(text, caret)`.
    fn delete(&mut self, cx: &mut Context<Self>, target: impl FnOnce(&str, usize) -> usize) {
        let Some(range) = self.selected_range() else {
            return;
        };
        let Some(open) = &self.open else {
            return;
        };
        let (range, kind) = if range.is_empty() {
            let other = target(&open.model.text, range.start);
            (
                range.start.min(other)..range.start.max(other),
                EditKind::Deleting,
            )
        } else {
            (range, EditKind::Other)
        };
        if !range.is_empty() {
            self.replace(range, "", kind, cx);
        }
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
        self.delete(cx, |text, caret| {
            if line_start == caret {
                movement::prev_grapheme(text, caret)
            } else {
                line_start
            }
        });
    }

    /// Enter, or Shift-Enter if `shift`: a line break that continues lists
    /// and quotes. See [`editing::enter`].
    fn newline(&mut self, shift: bool, cx: &mut Context<Self>) {
        let (Some(range), Some(open)) = (self.selected_range(), &self.open) else {
            return;
        };
        let model = &open.model;
        let index = model.item_at(range.start);
        // In a gap, Enter just adds blank lines.
        let kind = match model.gap_at(index, range.start) {
            Some(_) => BlockKind::Paragraph,
            None => model.items[index].kind,
        };
        let line_ending = line_ending(&model.text);
        let (range, text) = editing::enter(&model.text, kind, range, shift, line_ending);
        self.replace(range, &text, EditKind::Other, cx);
    }

    /// Tab, Shift-Tab, ⌘] or ⌘[: indents list items and code. See
    /// [`editing::indent`].
    fn indent(&mut self, outdent: bool, linewise: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(selection) = open.selection else {
            return;
        };
        let model = &open.model;
        let index = model.item_at(selection.range().start);
        if model.gap_at(index, selection.range().start).is_some() {
            return;
        }
        let item = &model.items[index];
        let Some(edit) = editing::indent(
            &model.text,
            item.kind,
            item.block.clone(),
            selection,
            outdent,
            linewise,
        ) else {
            return;
        };
        let after = edit.selection;
        self.replace(edit.range, &edit.text, EditKind::Other, cx);
        if let Some(open) = &mut self.open {
            open.history.set_selection_after(after);
        }
        self.set_selection(after, cx);
    }

    /// ⌥Tab: inserts a tab character.
    fn insert_tab(&mut self, _: &InsertTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selected_range() {
            self.replace(range, "\t", EditKind::Other, cx);
        }
    }

    /// Moves the caret to the start of the next or previous block's text.
    /// With no caret, starts at the first block on screen.
    fn move_to_block(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let model = &open.model;
        let target = match open.selection {
            None => Some(open.list_state.logical_scroll_top().item_ix),
            Some(selection) => {
                let index = model.item_at(selection.head);
                match (model.gap_at(index, selection.head), down) {
                    (Some(Gap::Above), true) | (Some(Gap::Below), false) => Some(index),
                    (_, true) => Some(index + 1).filter(|&next| next < model.items.len()),
                    (_, false) => index.checked_sub(1),
                }
            }
        };
        let Some(item) = target.and_then(|index| model.items.get(index)) else {
            return;
        };
        let offset = if item.block.is_empty() {
            item.range.start
        } else {
            editing::content_start(&model.text, item.block.clone(), item.kind)
        };
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.move_caret(offset, extend, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let (Some(range), Some(open)) = (self.selected_range(), &self.open) else {
            return;
        };
        if !range.is_empty() {
            let text = open.model.text[range].to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if let Some(range) = self.selected_range()
            && !range.is_empty()
        {
            self.replace(range, "", EditKind::Other, cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let (Some(range), Some(open)) = (self.selected_range(), &self.open) else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = normalize_line_endings(&text, line_ending(&open.model.text));
        self.replace(range, &text, EditKind::Other, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Some(selection) = open.history.undo(open.document.buffer_mut()) {
            open.marked = None;
            text_changed(open);
            self.set_selection(selection, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Some(selection) = open.history.redo(open.document.buffer_mut()) {
            open.marked = None;
            text_changed(open);
            self.set_selection(selection, cx);
        }
    }

    // UTF-16 conversion for the platform input handler.

    fn rope(&self) -> Option<&ropey::Rope> {
        Some(self.open.as_ref()?.document.buffer().rope())
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.rope().map_or(0, |rope| {
            rope.char_to_utf16_cu(rope.byte_to_char(offset.min(rope.len_bytes())))
        })
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        self.rope().map_or(0, |rope| {
            let offset = offset.min(rope.len_utf16_cu());
            rope.char_to_byte(rope.utf16_cu_to_char(offset))
        })
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

/// Reveals the items `selection` touches (or none), re-measuring the items
/// that change. Returns whether anything changed. With an empty selection
/// in a gap next to a block, the block stays rendered and the caret gets an
/// empty line of its own.
fn reveal(open: &mut OpenDocument, selection: Option<Selection>) -> bool {
    let model = &open.model;
    let targets: Vec<(usize, Range<usize>, Option<Gap>, bool)> = match selection {
        None => Vec::new(),
        Some(selection) => {
            let head = selection.head;
            let head_index = model.item_at(head);
            let range = selection.range();
            let first = model.item_at(range.start);
            let mut last = model.item_at(range.end);
            // A selection that ends where the next item starts doesn't
            // touch it.
            if last > first && last != head_index && model.items[last].range.start == range.end {
                last -= 1;
            }
            (first..=last)
                .map(|index| {
                    let is_head = index == head_index;
                    let gap = (is_head && selection.is_empty())
                        .then(|| model.gap_at(index, head))
                        .flatten();
                    let source = model.source_range(index);
                    let shown = match gap {
                        Some(_) => head..head,
                        // A selection can reach into the blank lines beyond
                        // the source; then show the whole item.
                        None if is_head && !(source.start..=source.end).contains(&head) => {
                            model.items[index].range.clone()
                        }
                        None => source,
                    };
                    (index, shown, gap, is_head)
                })
                .collect()
        }
    };
    let current: Vec<(usize, Range<usize>, Option<Gap>, bool)> = open
        .revealed
        .iter()
        .map(|revealed| {
            (
                revealed.index,
                revealed.range.clone(),
                revealed.gap,
                revealed.head,
            )
        })
        .collect();
    if current == targets {
        return false;
    }

    // Re-measure every item that's revealed now or was before.
    let mut changed: Vec<usize> = current
        .iter()
        .chain(&targets)
        .map(|(index, ..)| *index)
        .collect();
    changed.sort_unstable();
    changed.dedup();
    for index in changed {
        open.list_state.splice(index..index + 1, 1);
    }
    *open.revealed_layout.borrow_mut() = None;
    open.revealed = targets
        .into_iter()
        .map(|(index, range, gap, head)| {
            let kind = match gap {
                Some(_) => BlockKind::Paragraph,
                None => open.model.items[index].kind,
            };
            let source = &open.model.text[range.clone()];
            let styled = styled_source(source, kind, &open.model.definitions);
            Revealed {
                index,
                range,
                gap,
                head,
                styled: Rc::new(styled),
            }
        })
        .collect();
    true
}

/// Rebuilds the model after the buffer changed, re-measuring only the
/// items that changed. The caller then sets the selection, which reveals
/// its item again.
fn text_changed(open: &mut OpenDocument) {
    let text = open.document.buffer().rope().to_string();
    let (model, splice) = open.model.updated(&text);
    open.model = Rc::new(model);
    open.list_state.splice(splice.old, splice.new_count);
    // The revealed items' indices and sources may have changed.
    open.revealed.clear();
    *open.revealed_layout.borrow_mut() = None;
}

/// The line ending the document uses: CRLF if it has any, otherwise LF.
fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Converts all line endings in `text` to `line_ending`.
fn normalize_line_endings(text: &str, line_ending: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', line_ending)
}

impl EntityInputHandler for DocumentView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.open.as_ref()?.model.text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.open.as_ref()?.selection?;
        Some(UTF16Selection {
            range: self.range_to_utf16(&selection.range()),
            reversed: selection.head < selection.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let marked = self.open.as_ref()?.marked.clone()?;
        Some(self.range_to_utf16(&marked))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = &mut self.open {
            open.marked = None;
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
        let Some(open) = &self.open else {
            return;
        };
        let Some(range) = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or(open.marked.clone())
            .or(self.selected_range())
        else {
            return;
        };
        let kind = if range.is_empty() && open.marked.is_none() && !text.contains(['\n', '\r']) {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        self.replace(range, text, kind, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        let Some(range) = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or(open.marked.clone())
            .or(self.selected_range())
        else {
            return;
        };
        self.replace(range.clone(), text, EditKind::Other, cx);
        // The new selection is relative to the inserted text, in UTF-16.
        let base = self.offset_to_utf16(range.start);
        let selection = new_selected_range_utf16.map(|selected| Selection {
            anchor: self.offset_from_utf16(base + selected.start),
            head: self.offset_from_utf16(base + selected.end),
        });
        if let Some(open) = &mut self.open {
            open.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        }
        if let Some(selection) = selection {
            self.set_selection(selection, cx);
        }
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
                let model = open.model.clone();
                let selection = open.selection;
                let marked = open.marked.clone();
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
                    match revealed.iter().find(|revealed| revealed.index == index) {
                        Some(Revealed {
                            range,
                            gap,
                            head,
                            styled,
                            ..
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
                this.move_by(false, false, cx, movement::prev_grapheme)
            }))
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| {
                this.move_by(false, true, cx, movement::next_grapheme)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.move_by(true, false, cx, movement::prev_grapheme)
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_by(true, true, cx, movement::next_grapheme)
            }))
            .on_action(cx.listener(|this, _: &MoveWordLeft, _, cx| {
                this.move_by(false, false, cx, movement::prev_word_start)
            }))
            .on_action(cx.listener(|this, _: &MoveWordRight, _, cx| {
                this.move_by(false, true, cx, movement::next_word_end)
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.move_by(true, false, cx, movement::prev_word_start)
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.move_by(true, true, cx, movement::next_word_end)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentStart, _, cx| {
                this.move_by(false, false, cx, |_, _| 0)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentEnd, _, cx| {
                this.move_by(false, true, cx, |text, _| text.len())
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentStart, _, cx| {
                this.move_by(true, false, cx, |_, _| 0)
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentEnd, _, cx| {
                this.move_by(true, true, cx, |text, _| text.len())
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
            .on_action(
                cx.listener(|this, _: &Backspace, _, cx| this.delete(cx, movement::prev_grapheme)),
            )
            .on_action(
                cx.listener(|this, _: &Delete, _, cx| this.delete(cx, movement::next_grapheme)),
            )
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                this.delete(cx, movement::prev_word_start)
            }))
            .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| {
                this.delete(cx, movement::next_word_end)
            }))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(|this, _: &Newline, _, cx| this.newline(false, cx)))
            .on_action(cx.listener(|this, _: &LineBreak, _, cx| this.newline(true, cx)))
            .on_action(cx.listener(|this, _: &IndentTab, _, cx| this.indent(false, false, cx)))
            .on_action(cx.listener(|this, _: &OutdentTab, _, cx| this.indent(true, false, cx)))
            .on_action(cx.listener(|this, _: &IndentLines, _, cx| this.indent(false, true, cx)))
            .on_action(cx.listener(|this, _: &OutdentLines, _, cx| this.indent(true, true, cx)))
            .on_action(cx.listener(Self::insert_tab))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_line_ending() {
        assert_eq!(line_ending("a\nb"), "\n");
        assert_eq!(line_ending("a\r\nb"), "\r\n");
        assert_eq!(line_ending(""), "\n");
    }

    #[test]
    fn normalizes_pasted_line_endings() {
        assert_eq!(normalize_line_endings("a\r\nb\nc\rd", "\n"), "a\nb\nc\nd");
        assert_eq!(normalize_line_endings("a\nb\r\nc", "\r\n"), "a\r\nb\r\nc");
    }
}
