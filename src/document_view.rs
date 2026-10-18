//! The view that shows a document in a window.
//!
//! Every block is rendered except the one containing the caret, which is
//! revealed as styled Markdown source and edited in place (C4).
//!
//! The view's code is split by concern: opening and saving files
//! ([`files`]), the caret and selection ([`caret`]), editing and the
//! platform's text input ([`input`]), and drawing ([`render`]).

mod autosave;
mod caret;
mod disk;
mod files;
mod heights;
mod input;
mod line_scroll;
mod render;
mod scrollbar;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, KeyBinding, ListAlignment, ListState, Pixels, Subscription,
    SystemWindowTabController, Task, Window, WindowHandle, actions, px,
};
use kertas_core::document::{DiskChange, Document};
use kertas_core::editor::{Editor, RevealTarget};
use kertas_core::model::Splice;
use kertas_core::reveal::StyledSource;
use kertas_core::titles;

use crate::app_settings::{self, AppSettings};
use crate::builtin::Builtin;
use crate::menus::OpenSettings;
use crate::revealed_block::{CaretPlace, RevealedLayout};
use crate::text_style::TextSettings;
use crate::theme::Theme;
use crate::windows;
use crate::zoom::Zoom;

actions!(
    kertas,
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
    /// What to bring on screen in the next frame. Rendering takes it.
    scroll_request: Option<ScrollRequest>,
    /// The file being watched for changes by other apps, and the task
    /// that handles them. Dropping it stops watching.
    watched: Option<(PathBuf, Task<()>)>,
    /// A change to the file that the bar at the top is asking about.
    disk_change: Option<DiskChange>,
    /// Set when an autosave fails, until the user saves the document.
    autosave_paused: bool,
    /// The scrollbar's estimate of the items' heights, kept until the
    /// document or its line width changes.
    scroll_estimate: RefCell<Option<scrollbar::CachedEstimate>>,
    /// A drag of the scrollbar's thumb in progress.
    scrollbar_drag: Option<scrollbar::ScrollbarDrag>,
}

/// Something to bring on screen. The element that draws it asks the list
/// to scroll to it as the frame is laid out (`Window::request_autoscroll`),
/// so it's on screen in that same frame, rather than a frame late.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ScrollRequest {
    /// The caret's line, with a line of context above and below, and its
    /// item at `CaretPlace`.
    Caret(CaretPlace),
    /// The line holding document offset `offset`, to the top of the
    /// window: the footnote reference a note's back-link went to (#90).
    Position(usize),
}

impl OpenDocument {
    /// Updates the list after the model changes: the items `splice`
    /// replaced, and the footnotes after them if they look different.
    fn splice(&self, splice: &Splice) {
        self.list_state.splice(splice.old.clone(), splice.new_count);
        if let Some((old, new)) = splice.notes {
            let start = self.editor.model().items.len();
            if old == new {
                // Keeping their heights while they're measured again.
                self.list_state.remeasure_items(start..start + new);
            } else {
                self.list_state.splice(start..start + old, new);
            }
        }
    }
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

/// Whether a document's web images are loaded (#97).
#[derive(Clone, Copy, PartialEq, Eq)]
enum WebImages {
    /// Not yet: the bar asks, if the document has any.
    Ask,
    /// The user closed the bar, so they aren't.
    Declined,
    /// The user chose Load Images.
    Load,
}

pub struct DocumentView {
    focus_handle: FocusHandle,
    open: Option<OpenDocument>,
    /// Set when showing one of Kertas's built-in documents, which are
    /// view-only.
    builtin: Option<Builtin>,
    /// Images still loading in the document, whose blocks need measuring
    /// again once they arrive.
    loading_images: crate::rendered_block::LoadingImages,
    /// Whether the document's web images are loaded (#97). It's asked
    /// each time a document is opened, and never remembered.
    web_images: WebImages,
    /// The folder shown after the file's name in the title, if another
    /// open file has the same name.
    title_hint: Option<String>,
    theme: Theme,
    /// The settings the document was last laid out with: the text width,
    /// tab width and fonts.
    text_settings: TextSettings,
    /// Whether the blinking caret is currently drawn.
    caret_visible: bool,
    /// A scroll from ↑ or ↓ in progress, with no caret.
    line_scroll: Option<line_scroll::LineScroll>,
    /// Bumped to stop the current blink loop.
    blink_epoch: usize,
    /// Whether the mouse button went down in the document and is still
    /// down, so moving it extends the selection.
    dragging: bool,
    /// The checkbox the mouse button went down on, by where its `[ ]`
    /// starts, so it toggles when the button goes up on it.
    pressed_task: Option<usize>,
    /// Whether a "save changes?" prompt is already showing.
    confirming: bool,
    /// The edited state last shown in the window's close button.
    shown_edited: Option<bool>,
    /// Whether the user closed the bar reporting problems in the settings
    /// file. It shows again when the file changes.
    settings_problems_dismissed: bool,
    /// This view's window, to autosave in it later.
    window_handle: Option<WindowHandle<DocumentView>>,
    /// Bumped by each edit, so only the last one's autosave timer saves.
    autosave_epoch: usize,
    _appearance_observer: Subscription,
    _activation_observer: Subscription,
    _settings_observer: Subscription,
    _zoom_observer: Subscription,
    _tabs_observer: Subscription,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A tab stop, so Tab can come back to the document from the bars'
        // buttons.
        let focus_handle = cx.focus_handle().tab_stop(true);
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
            // Turning autosave on saves changes already made.
            if this.is_dirty() {
                this.schedule_autosave(cx);
            }
            this.settings_problems_dismissed = false;
            let text_settings = TextSettings::from(&app_settings::get(cx));
            if text_settings != this.text_settings {
                this.text_settings = text_settings;
                this.layout_changed();
            }
            cx.notify();
        });
        // Autosave when the window or tab loses focus.
        let activation_observer = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                // The default Markdown app can change in Finder.
                crate::menus::refresh(cx);
            } else {
                this.autosave(window, cx);
            }
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
            title_hint: None,
            loading_images: Rc::default(),
            web_images: WebImages::Ask,
            theme: Theme::current(window, cx),
            text_settings: TextSettings::from(&app_settings::get(cx)),
            caret_visible: true,
            line_scroll: None,
            dragging: false,
            pressed_task: None,
            blink_epoch: 0,
            confirming: false,
            shown_edited: None,
            settings_problems_dismissed: false,
            window_handle: window.window_handle().downcast(),
            autosave_epoch: 0,
            _appearance_observer: appearance_observer,
            _activation_observer: activation_observer,
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
        self.web_images = WebImages::Ask;
        // The footnotes follow the items (#90).
        let items = editor.model().items.len() + editor.model().notes.len();
        let list_state = ListState::new(items, ListAlignment::Top, px(1000.));
        // Short documents are measured up front, so the scrollbar is exact.
        let list_state = if items <= heights::MEASURE_ALL_LIMIT {
            list_state.measure_all()
        } else {
            list_state
        };
        self.open = Some(OpenDocument {
            editor,
            list_state,
            goal_x: None,
            revealed: Vec::new(),
            revealed_layout: Rc::default(),
            scroll_request: None,
            watched: None,
            disk_change: None,
            autosave_paused: false,
            scroll_estimate: RefCell::default(),
            scrollbar_drag: None,
        });
        self.watch_file(window, cx);
        self.stop_blink();
        self.update_window_title(window, cx);
        cx.notify();
    }

    /// Updates the window's title after its document changes file, and
    /// every other window's, since they may share a name (#95).
    fn update_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_window_title(window, cx);
        cx.defer(windows::refresh_titles);
    }

    /// Sets the folder shown after the file's name, to tell it from other
    /// open files with the same name.
    pub fn set_title_hint(
        &mut self,
        hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.title_hint != hint {
            self.title_hint = hint;
            self.apply_window_title(window, cx);
        }
    }

    fn apply_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let name = match self.builtin {
            Some(builtin) => builtin.title().to_owned(),
            None => titles::title(
                &open.editor.document().display_name(),
                self.title_hint.as_deref(),
            ),
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
    pub fn reveal_state(&self) -> (usize, Option<kertas_core::editor::Selection>) {
        self.open.as_ref().map_or((0, None), |open| {
            (open.revealed.len(), open.editor.selection())
        })
    }

    /// Whether the caret's line is inside the window, for the smoke test.
    /// `None` without a caret, `Some(false)` if its block isn't laid out.
    #[cfg(debug_assertions)]
    pub fn caret_on_screen(&self) -> Option<bool> {
        let open = self.open.as_ref()?;
        let selection = open.editor.selection()?;
        let viewport = open.list_state.viewport_bounds();
        let index = open.editor.model().item_at(selection.head);
        let Some(bounds) = open.list_state.bounds_for_item(index) else {
            return Some(false);
        };
        if let Some((layout, start)) = self.revealed_layout()
            && let Some(position) = layout.position_for_index(selection.head.saturating_sub(start))
        {
            return Some(
                position.y >= viewport.top()
                    && position.y + layout.line_height() <= viewport.bottom(),
            );
        }
        Some(bounds.top() < viewport.bottom() && bounds.bottom() > viewport.top())
    }

    /// The index of the item holding document `offset`, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    pub fn item_at(&self, offset: usize) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| open.editor.model().item_at(offset))
    }

    /// The first footnote's row and how many there are, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    pub fn note_rows(&self) -> (usize, usize) {
        self.open.as_ref().map_or((0, 0), |open| {
            let model = open.editor.model();
            (model.items.len(), model.notes.len())
        })
    }

    /// Whether row `index` is at least partly inside the window, for the
    /// smoke test.
    #[cfg(debug_assertions)]
    pub fn row_on_screen(&self, index: usize) -> bool {
        self.open.as_ref().is_some_and(|open| {
            heights::Heights::measured(&open.list_state)
                .on_screen(index)
                .is_some()
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

    /// The list indent the document is laid out with, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn list_indent(&self) -> kertas_core::settings::ListIndent {
        self.text_settings.list_indent
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

    /// Whether the bar reporting problems in the settings file is showing,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    pub fn settings_problems_shown(&self) -> bool {
        !self.settings_problems_dismissed
    }

    /// The document's model, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn model(&self) -> Option<Rc<kertas_core::model::Model>> {
        self.open.as_ref().map(|open| open.editor.model().clone())
    }
}
