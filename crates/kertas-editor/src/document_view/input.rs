//! Editing: the edit actions, the clipboard, undo, and the platform's text
//! input (typing and input methods).

use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, px,
    size,
};
use kertas_core::editor::{Change, Editor};

use super::{Copy, Cut, DeleteToLineStart, DocumentView, Paste, Redo, Undo};

impl DocumentView {
    // Editing

    /// Makes an edit with the editor and shows the result. The caret's
    /// goal x position is reset.
    pub(super) fn edit(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Editor) -> Option<Change>,
    ) {
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
        self.close_hover(cx);
        let Some(open) = &mut self.open else {
            return;
        };
        open.splice(&change.splice);
        // The revealed items' indices and sources may have changed.
        open.revealed.clear();
        *open.revealed_layout.borrow_mut() = None;
        if open.editor.selection().is_some() {
            self.set_selection(change.selection, cx);
        } else {
            // An edit with no caret, such as Replace All (#123).
            super::caret::reveal(open);
            cx.notify();
        }
        self.schedule_autosave(cx);
        self.schedule_edited(cx);
    }

    /// Replaces each of `edits`' byte ranges of the text with its new text,
    /// as one edit that one Undo takes back (#125), such as replacing
    /// across a host's documents. The ranges are in the text as it is now,
    /// on character boundaries, and mustn't overlap. The selection keeps
    /// its place in the text around them; with no caret, nothing starts
    /// editing. Returns false, changing nothing, if the ranges aren't
    /// valid, there are none, or the document is read-only.
    pub fn replace_ranges(
        &mut self,
        edits: Vec<(Range<usize>, String)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = &mut self.open else {
            return false;
        };
        let Some(change) = open.editor.replace_ranges(edits) else {
            return false;
        };
        open.goal_x = None;
        self.apply(change, cx);
        true
    }

    /// Notes that the mouse went down on the checkbox whose `[ ]` starts
    /// at document offset `marker`.
    pub(crate) fn press_task(&mut self, marker: usize) {
        self.pressed_task = Some(marker);
    }

    /// Toggles the checkbox at `marker` when the mouse goes up on the one
    /// it went down on. The selection stays as it is, so the list stays
    /// rendered.
    pub(crate) fn release_task(&mut self, marker: usize, cx: &mut Context<Self>) {
        if self.pressed_task.take() == Some(marker) {
            self.toggle_task(marker, cx);
        }
    }

    /// Toggles the checkbox whose `[ ]` starts at document offset
    /// `marker`, leaving the selection as it is.
    pub(crate) fn toggle_task(&mut self, marker: usize, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(splice) = open.editor.toggle_task(marker) else {
            return;
        };
        open.splice(&splice);
        super::caret::reveal(open);
        self.schedule_autosave(cx);
        self.schedule_edited(cx);
        cx.notify();
    }

    pub(super) fn delete_to_line_start(
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

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self
            .open
            .as_ref()
            .and_then(|open| open.editor.selected_text())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.edit(cx, Editor::cut);
    }

    pub(super) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if self.paste_images(&item, cx) {
            return;
        }
        let Some(text) = item.text() else {
            return;
        };
        self.edit(cx, |editor| editor.paste(&text));
    }

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(change) = self.open.as_mut().and_then(|open| open.editor.undo()) {
            self.apply(change, cx);
        }
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
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
