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
    Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, KeyBinding, ListAlignment,
    ListState, PathPromptOptions, Pixels, Point, PromptLevel, Subscription, Task, TextLayout,
    UTF16Selection, Window, actions, div, list, point, prelude::*, px, relative, size,
};
use kami::document::{Document, OpenError, SaveError};
use kami::history::{EditKind, History};
use kami::model::Model;
use kami::movement::{self, Selection};
use kami::reveal::{StyledSource, styled_source};

use crate::rendered_block::{ClickTarget, render_block};
use crate::revealed_block::{RevealedBlock, TextInput, render_revealed};
use crate::theme::Theme;

actions!(
    kami,
    [
        New,
        Open,
        Save,
        SaveAs,
        CloseWindow,
        Quit,
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
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
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
        KeyBinding::new("shift-enter", Newline, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
    ]
}

/// Maximum width of the text column (A1).
pub const MAX_TEXT_WIDTH: f32 = 720.;
pub const BODY_TEXT_SIZE: f32 = 16.;
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
    revealed: Option<Revealed>,
    /// The revealed item's text layout from the last frame.
    revealed_layout: Rc<RefCell<Option<TextLayout>>>,
}

/// The item shown as source.
struct Revealed {
    index: usize,
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
    theme: Theme,
    /// Whether the blinking caret is currently drawn.
    caret_visible: bool,
    /// Bumped to stop the current blink loop.
    blink_epoch: usize,
    /// Whether a "save changes?" prompt is already showing.
    confirming: bool,
    /// The edited state last shown in the window's close button.
    shown_edited: Option<bool>,
    _appearance_observer: Subscription,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        // Follow the macOS appearance, switching live when it changes.
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = Theme::for_appearance(window.appearance());
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
            theme: Theme::for_appearance(window.appearance()),
            caret_visible: true,
            blink_epoch: 0,
            confirming: false,
            shown_edited: None,
            _appearance_observer: appearance_observer,
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
            revealed: None,
            revealed_layout: Rc::default(),
        });
        self.stop_blink();
        self.update_window_title(window);
        cx.notify();
    }

    fn update_window_title(&mut self, window: &mut Window) {
        let Some(open) = &self.open else {
            return;
        };
        window.set_window_title(&open.document.display_name());
        window.set_document_path(open.document.path());
    }

    fn is_dirty(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.document.is_dirty())
    }

    // Files

    fn new_document(&mut self, _: &New, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if confirmed.await {
                this.update_in(cx, |this, window, cx| {
                    this.load(Document::untitled(), window, cx);
                    // Start typing straight away.
                    this.move_caret(0, false, cx);
                })
                .ok();
            }
        })
        .detach();
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
                self.update_window_title(window);
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
        match Document::open(path) {
            Ok(document) => self.load(document, window, cx),
            Err(err) => show_open_error(&err, window, cx),
        }
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !confirmed.await {
                return;
            }
            let Ok(paths) = this.update(cx, |_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: None,
                })
            }) else {
                return;
            };
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
        self.move_caret(offset, extend, cx);
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
        let Some(open) = &mut self.open else {
            return;
        };
        open.selection = Some(selection);
        let index = open.model.item_at(selection.head);
        reveal(open, Some(index));
        open.list_state.scroll_to_reveal_item(index);
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
        let Some(open) = &mut self.open else {
            return;
        };
        open.selection = None;
        open.goal_x = None;
        open.marked = None;
        open.history.break_group();
        reveal(open, None);
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
        let revealed = open.revealed.as_ref()?;
        let layout = open.revealed_layout.borrow().clone()?;
        Some((layout, open.model.items[revealed.index].range.start))
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

        // Past the item's edge: reveal the neighbor, then place the caret on
        // its first or last line once it's laid out.
        let index = open.model.item_at(caret);
        let neighbor = if down {
            open.model.items.get(index + 1).map(|item| item.range.start)
        } else {
            index
                .checked_sub(1)
                .map(|index| open.model.items[index].range.end)
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

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        let (Some(range), Some(open)) = (self.selected_range(), &self.open) else {
            return;
        };
        let line_ending = line_ending(&open.model.text);
        self.replace(range, line_ending, EditKind::Other, cx);
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

/// Reveals item `index` (or none), re-measuring the items that change.
fn reveal(open: &mut OpenDocument, index: Option<usize>) {
    if open.revealed.as_ref().map(|revealed| revealed.index) == index {
        return;
    }
    if let Some(old) = open.revealed.take() {
        open.list_state.splice(old.index..old.index + 1, 1);
    }
    *open.revealed_layout.borrow_mut() = None;
    if let Some(index) = index {
        let item = &open.model.items[index];
        let source = &open.model.text[item.range.clone()];
        let styled = styled_source(source, item.kind, &open.model.definitions);
        open.revealed = Some(Revealed {
            index,
            styled: Rc::new(styled),
        });
        open.list_state.splice(index..index + 1, 1);
    }
}

/// Rebuilds the model after the buffer changed, re-measuring only the
/// items that changed. The caller then sets the selection, which reveals
/// its item again.
fn text_changed(open: &mut OpenDocument) {
    let text = open.document.buffer().rope().to_string();
    let (model, splice) = open.model.updated(&text);
    open.model = Rc::new(model);
    open.list_state.splice(splice.old, splice.new_count);
    // The revealed item's index and source may have changed.
    open.revealed = None;
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

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Show unsaved changes in the window's close button.
        let edited = self.is_dirty();
        if self.shown_edited != Some(edited) {
            window.set_window_edited(edited);
            self.shown_edited = Some(edited);
        }

        let content = match &self.open {
            Some(open) => {
                let model = open.model.clone();
                let selection = open.selection;
                let marked = open.marked.clone();
                let revealed = open
                    .revealed
                    .as_ref()
                    .map(|revealed| (revealed.index, revealed.styled.clone()));
                let layout_slot = open.revealed_layout.clone();
                let caret_visible = self.caret_visible;
                let base_font = window.text_style().font();
                let theme = self.theme;
                let view = cx.entity();
                let focus_handle = self.focus_handle.clone();
                list(open.list_state.clone(), move |index, _, _| {
                    let item = &model.items[index];
                    match &revealed {
                        Some((revealed_index, styled)) if *revealed_index == index => {
                            let range = item.range.clone();
                            let selection = selection.unwrap_or_default();
                            let local =
                                |offset: usize| offset.clamp(range.start, range.end) - range.start;
                            let selected = selection.range();
                            let caret = (caret_visible
                                && (range.contains(&selection.head)
                                    || selection.head == range.end))
                                .then(|| selection.head - range.start);
                            let marked = marked
                                .as_ref()
                                .map(|marked| local(marked.start)..local(marked.end));
                            render_revealed(
                                RevealedBlock {
                                    source: &model.text[range.clone()],
                                    styled,
                                    selection: local(selected.start)..local(selected.end),
                                    caret,
                                    marked,
                                    first: index == 0,
                                },
                                &base_font,
                                theme,
                                ClickTarget {
                                    view: view.downgrade(),
                                    block_start: range.start,
                                },
                                layout_slot.clone(),
                                TextInput {
                                    focus_handle: focus_handle.clone(),
                                    view: view.clone(),
                                },
                            )
                        }
                        _ => render_block(
                            &item.rendered,
                            index == 0,
                            &base_font,
                            theme,
                            ClickTarget {
                                view: view.downgrade(),
                                block_start: item.block.start,
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
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::unreveal))
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
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .size_full()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(px(BODY_TEXT_SIZE))
            .line_height(relative(1.6))
            .child(content)
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
