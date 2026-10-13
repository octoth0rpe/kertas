//! The view that shows a document in a window.
//!
//! Every block is rendered except the one containing the caret, which is
//! revealed as styled Markdown source and edited in place (C4). See
//! `docs/planning/caret-and-reveal.md`.
//!
//! The view's code is split by concern: opening and saving files
//! ([`files`]), the caret and selection ([`caret`]), editing and the
//! platform's text input ([`input`]), and drawing ([`render`]).

mod caret;
mod disk;
mod files;
mod input;
mod render;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, KeyBinding, ListAlignment, ListState, Pixels, Subscription,
    SystemWindowTabController, Task, Window, actions, px,
};
use kami::document::{DiskChange, Document};
use kami::editor::{Editor, RevealTarget};
use kami::reveal::StyledSource;

use crate::app_settings::{self, AppSettings};
use crate::builtin::Builtin;
use crate::menus::OpenSettings;
use crate::revealed_block::RevealedLayout;
use crate::text_style::TextSettings;
use crate::theme::Theme;
use crate::zoom::Zoom;

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
    revealed_layout: Rc<RefCell<Option<RevealedLayout>>>,
    /// An item to keep at the same place on screen once the next frame is
    /// laid out, and where it was (C7): revealing blocks changes their
    /// heights, which would otherwise move the text under the pointer.
    anchor: Option<(usize, Pixels)>,
    /// Whether to scroll the caret into view once the next frame is laid
    /// out.
    reveal_caret: bool,
    /// The file being watched for changes by other apps, and the task
    /// that handles them. Dropping it stops watching.
    watched: Option<(PathBuf, Task<()>)>,
    /// A change to the file that the bar at the top is asking about.
    disk_change: Option<DiskChange>,
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
    /// The settings the document was last laid out with: the text width,
    /// tab width and fonts.
    text_settings: TextSettings,
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
            let text_settings = TextSettings::from(&app_settings::get(cx));
            if text_settings != this.text_settings {
                this.text_settings = text_settings;
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
            text_settings: TextSettings::from(&app_settings::get(cx)),
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
            watched: None,
            disk_change: None,
        });
        self.watch_file(window, cx);
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
    pub fn reveal_state(&self) -> (usize, Option<kami::editor::Selection>) {
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

    /// The document's model, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn model(&self) -> Option<Rc<kami::model::Model>> {
        self.open.as_ref().map(|open| open.editor.model().clone())
    }
}
