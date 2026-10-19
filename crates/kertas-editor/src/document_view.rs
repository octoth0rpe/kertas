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
mod fade;
mod files;
mod find;
mod heights;
mod hover;
mod input;
mod line_scroll;
mod marking;
mod render;
mod scrollbar;
mod spelling;

pub use files::HeadingTarget;
pub use hover::{HOVER_DELAY, HoverPopover, HoverTarget};
pub use render::bar_button;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyWindowHandle, Context, EventEmitter, FocusHandle, Focusable, KeyBinding, ListAlignment,
    ListState, Pixels, Subscription, Task, Window, px,
};
use kertas_core::document::{DiskChange, Document};
use kertas_core::editor::{Editor, RevealTarget};
use kertas_core::model::Splice;
use kertas_core::reveal::StyledSource;
use kertas_core::{outline, words};

use crate::editor_config::EditorConfig;
use crate::marks::Mark;
use crate::revealed_block::{CaretPlace, RevealedLayout};
use crate::theme::Theme;

pub use crate::actions::*;

/// Key bindings for the document view (macOS conventions), its find bar,
/// text fields and context menus.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
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
        KeyBinding::new("cmd-f", Find, None),
        KeyBinding::new("cmd-g", FindNext, None),
        KeyBinding::new("cmd-shift-g", FindPrevious, None),
        KeyBinding::new("cmd-e", UseSelectionForFind, None),
        KeyBinding::new("alt-cmd-f", FindAndReplace, None),
    ]
    .into_iter()
    .chain(find::key_bindings())
    .chain(crate::text_field::key_bindings())
    .chain(crate::context_menu::key_bindings())
    .collect()
}

/// How long the caret stays on, and then off, while blinking. Public so
/// other text controls in a host app can blink in step with the editor.
pub const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// How long the text must stay unchanged before [`DocumentEvent::Edited`]
/// is sent.
pub const EDIT_PAUSE: Duration = Duration::from_millis(300);

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
    /// The line holding the find match that starts at `offset`, into view
    /// (#122).
    Match(usize),
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

/// What a document view tells the window it's in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentEvent {
    /// The document now has a different file or name: it was opened,
    /// started, or saved to another file.
    TitleChanged,
    /// The document now has unsaved changes, or no longer does.
    DirtyChanged(bool),
    /// The text changed: it was edited, or reloaded from its file. It's
    /// sent once changes pause for [`EDIT_PAUSE`], so the app can keep
    /// something like an outline or a word count in step with the text
    /// without working on every keystroke.
    Edited,
    /// The caret moved to another place, or went away, without an edit
    /// (see [`DocumentView::caret_offset`]). An edit that moves it sends
    /// [`Edited`](Self::Edited) instead, after the pause.
    CaretMoved,
    /// A link to `destination`, as written, was clicked, and the host
    /// decides what it opens. `wiki` says it's a `[[wiki link]]`, whose
    /// destination is its target. It's sent only once the host asks for it
    /// with [`DocumentView::set_links_to_host`]; otherwise links are
    /// followed.
    LinkClicked { destination: String, wiki: bool },
    /// A link to another Markdown file was followed, and asks for it to be
    /// opened, scrolled to the heading named `anchor` if there is one.
    OpenFile {
        path: PathBuf,
        anchor: Option<String>,
    },
    /// The text the host marked with `id` (see
    /// [`DocumentView::set_marks`]), now at `range`, was clicked with the
    /// right button or Control-clicked at `position` in the window. The
    /// host can show a menu there, such as spelling suggestions.
    MarkMenu {
        id: usize,
        range: std::ops::Range<usize>,
        position: gpui::Point<Pixels>,
    },
}

pub struct DocumentView {
    focus_handle: FocusHandle,
    open: Option<OpenDocument>,
    /// The name of text shown view-only, which has no file (see
    /// [`DocumentView::show_text`]).
    text_name: Option<String>,
    /// Images still loading in the document, whose blocks need measuring
    /// again once they arrive.
    loading_images: crate::rendered_block::LoadingImages,
    /// Whether the document's web images are loaded (#97). It's asked
    /// each time a document is opened, and never remembered.
    web_images: WebImages,
    /// What the app last told the view, which the document is laid out
    /// with.
    config: EditorConfig,
    /// The config's theme, in the window's light or dark colors.
    theme: Theme,
    /// Whether the blinking caret is currently drawn.
    caret_visible: bool,
    /// Which items are faded, while one is being edited (#121).
    fade: fade::Fade,
    /// A scroll from ↑ or ↓ in progress, with no caret.
    line_scroll: Option<line_scroll::LineScroll>,
    /// Bumped to stop the current blink loop.
    blink_epoch: usize,
    /// Whether the mouse button went down in the document and is still
    /// down, so moving it extends the selection.
    dragging: bool,
    /// Whether clicked links go to the host rather than being followed.
    links_to_host: bool,
    /// The host's check of which links lead anywhere.
    link_check: Option<crate::rendered_block::LinkCheck>,
    /// The ranges the host marked, in order (#120).
    marks: Rc<Vec<Mark>>,
    /// The text the marks were last moved to, while there are any. When
    /// the document's text is another, they're moved to it.
    marks_model: Option<Rc<kertas_core::model::Model>>,
    /// The misspelled words (#141).
    misspelled: spelling::Misspelled,
    /// The menu of a misspelled word, while it's open (#142).
    spelling_menu: Option<spelling::SpellingMenu>,
    /// What the host shows when the pointer rests on a link or a mark
    /// (#126).
    hover_popover: Option<HoverPopover>,
    /// The link or mark the pointer is on, and its popover.
    hover: Option<hover::Hover>,
    /// Bumped to stop the popover's timers.
    hover_epoch: usize,
    /// The find bar, and what it found (#122).
    find: find::FindState,
    /// The checkbox the mouse button went down on, by where its `[ ]`
    /// starts, so it toggles when the button goes up on it.
    pressed_task: Option<usize>,
    /// Whether a "save changes?" prompt is already showing.
    confirming: bool,
    /// The unsaved changes state last reported with
    /// [`DocumentEvent::DirtyChanged`].
    reported_dirty: Option<bool>,
    /// This view's window, to autosave in it later.
    window_handle: AnyWindowHandle,
    /// Bumped by each edit, so only the last one's autosave timer saves.
    autosave_epoch: usize,
    /// Bumped by each change to the text, so only the last one's timer
    /// sends [`DocumentEvent::Edited`].
    edited_epoch: usize,
    _appearance_observer: Subscription,
    _activation_observer: Subscription,
    /// Checks the spelling again when words are ignored or added.
    _spelling_observer: Subscription,
}

impl EventEmitter<DocumentEvent> for DocumentView {}

impl Focusable for DocumentView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentView {
    pub fn new(config: EditorConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A tab stop, so Tab can come back to the document from the bars'
        // buttons.
        let focus_handle = cx.focus_handle().tab_stop(true);
        focus_handle.focus(window, cx);
        // Follow the macOS appearance, switching live when it changes.
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = Theme::for_window(&this.config.theme, this.config.appearance, window);
            cx.notify();
        });
        // Autosave when the window or tab loses focus.
        let activation_observer = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.autosave(window, cx);
            }
            // Nothing is faded while the window is in the background.
            cx.notify();
        });
        let spelling_observer = crate::spelling::observe(cx, |this, cx| this.check_spelling(cx));
        Self {
            focus_handle,
            open: None,
            text_name: None,
            loading_images: Rc::default(),
            web_images: WebImages::Ask,
            theme: Theme::for_window(&config.theme, config.appearance, window),
            config,
            caret_visible: true,
            fade: fade::Fade::default(),
            line_scroll: None,
            dragging: false,
            links_to_host: false,
            link_check: None,
            marks: Rc::default(),
            marks_model: None,
            misspelled: Default::default(),
            spelling_menu: None,
            hover_popover: None,
            hover: None,
            hover_epoch: 0,
            find: find::FindState::default(),
            pressed_task: None,
            blink_epoch: 0,
            confirming: false,
            reported_dirty: None,
            window_handle: window.window_handle(),
            autosave_epoch: 0,
            edited_epoch: 0,
            _appearance_observer: appearance_observer,
            _activation_observer: activation_observer,
            _spelling_observer: spelling_observer,
        }
    }

    /// Applies a new configuration from the app: when the settings or the
    /// zoom change.
    pub fn set_config(
        &mut self,
        config: EditorConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if config == self.config {
            return;
        }
        let relayout = config.text != self.config.text || config.font_size != self.config.font_size;
        let autosave_changed = config.autosave != self.config.autosave;
        let syntax_changed = config.syntax() != self.config.syntax();
        let spelling_changed = config.check_spelling != self.config.check_spelling
            || config.spelling_language != self.config.spelling_language;
        let find_options_changed = config.find_options != self.config.find_options;
        self.theme = Theme::for_window(&config.theme, config.appearance, window);
        self.config = config;
        // Turning autosave on saves changes already made.
        if autosave_changed && self.is_dirty() {
            self.schedule_autosave(cx);
        }
        if syntax_changed
            && let Some(open) = &mut self.open
            && let Some(splice) = open.editor.set_syntax(self.config.syntax())
        {
            open.splice(&splice);
            caret::reveal(open);
        }
        if relayout || syntax_changed {
            self.layout_changed();
        }
        if find_options_changed {
            self.find_options_offered(cx);
        }
        if spelling_changed || syntax_changed {
            self.check_spelling(cx);
        }
        cx.notify();
    }

    /// Shows `document`, replacing the current one.
    fn load(&mut self, document: Document, window: &mut Window, cx: &mut Context<Self>) {
        // Named text has no file, and is view-only.
        let editor = Editor::new_with(document, self.text_name.is_some(), self.config.syntax());
        self.web_images = WebImages::Ask;
        // Marks were for the document this one replaces.
        self.marks = Rc::default();
        self.marks_model = None;
        self.forget_matches();
        self.close_hover(cx);
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
        self.misspelled = Default::default();
        self.check_spelling(cx);
        cx.emit(DocumentEvent::TitleChanged);
        cx.notify();
    }

    /// The document's name: its file's name, the name of text shown
    /// view-only, or Untitled. `None` with no document.
    pub fn display_name(&self) -> Option<String> {
        let open = self.open.as_ref()?;
        Some(match &self.text_name {
            Some(name) => name.clone(),
            None => open.editor.document().display_name(),
        })
    }

    /// The configuration the view was last given.
    pub fn config(&self) -> &EditorConfig {
        &self.config
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
    #[doc(hidden)]
    pub fn item_count(&self) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| open.editor.model().items.len())
    }

    /// Where item `index`'s block starts in the document, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
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
    #[doc(hidden)]
    pub fn item_position(&self, index: usize) -> Option<(usize, Pixels)> {
        let open = self.open.as_ref()?;
        let item = open.editor.model().items.get(index)?;
        let top = open.list_state.bounds_for_item(index)?.top();
        Some((item.block.start, top))
    }

    /// How many items are revealed, and the selection, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn reveal_state(&self) -> (usize, Option<kertas_core::editor::Selection>) {
        self.open.as_ref().map_or((0, None), |open| {
            (open.revealed.len(), open.editor.selection())
        })
    }

    /// Whether the caret's line is inside the window, for the smoke test.
    /// `None` without a caret, `Some(false)` if its block isn't laid out.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
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
    #[doc(hidden)]
    pub fn item_at(&self, offset: usize) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| open.editor.model().item_at(offset))
    }

    /// The first footnote's row and how many there are, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn note_rows(&self) -> (usize, usize) {
        self.open.as_ref().map_or((0, 0), |open| {
            let model = open.editor.model();
            (model.items.len(), model.notes.len())
        })
    }

    /// Whether row `index` is at least partly inside the window, for the
    /// smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn row_on_screen(&self, index: usize) -> bool {
        self.open.as_ref().is_some_and(|open| {
            heights::Heights::measured(&open.list_state)
                .on_screen(index)
                .is_some()
        })
    }

    /// The list's scroll position, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn scroll_top(&self) -> Option<(usize, Pixels)> {
        let top = self.open.as_ref()?.list_state.logical_scroll_top();
        Some((top.item_ix, top.offset_in_item))
    }

    /// The document's text. Empty with no document.
    pub fn text(&self) -> String {
        self.open
            .as_ref()
            .map(|open| open.editor.text().to_owned())
            .unwrap_or_default()
    }

    /// The document's headings, in order (see [`outline::headings`]).
    /// None with no document.
    pub fn headings(&self) -> Vec<outline::Heading> {
        self.open
            .as_ref()
            .map(|open| outline::headings(open.editor.model()))
            .unwrap_or_default()
    }

    /// How many words the document has (see [`words::count_in`]). 0 with
    /// no document.
    pub fn word_count(&self) -> usize {
        self.open
            .as_ref()
            .map_or(0, |open| words::count_in(open.editor.model()))
    }

    /// Sends [`DocumentEvent::Edited`] after the [`EDIT_PAUSE`], unless the
    /// text changes again first.
    fn schedule_edited(&mut self, cx: &mut Context<Self>) {
        self.edited_epoch += 1;
        let epoch = self.edited_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EDIT_PAUSE).await;
            this.update(cx, |view, cx| {
                if view.edited_epoch == epoch {
                    // So the host reads them where the text now has them.
                    view.move_marks();
                    view.check_spelling(cx);
                    cx.emit(DocumentEvent::Edited);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The list indent the document is laid out with, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn list_indent(&self) -> kertas_core::settings::ListIndent {
        self.config.text.list_indent
    }

    /// The colors the view is drawn in.
    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_dirty(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.editor.is_dirty())
    }

    /// The document's model, e.g. to print it.
    pub fn model(&self) -> Option<Rc<kertas_core::model::Model>> {
        self.open.as_ref().map(|open| open.editor.model().clone())
    }

    /// Whether the user chose to load the document's web images.
    pub fn web_images_loaded(&self) -> bool {
        self.web_images == WebImages::Load
    }
}
