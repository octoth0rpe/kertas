//! The editor behind a document window: the text, the selection, undo and
//! the input method's marked text, and every edit made to them.
//!
//! Everything here is independent of layout. The view keeps what depends
//! on it (scrolling, visual lines, the caret's x position), asks the editor
//! for edits, and applies the [`Change`] each one returns to its list of
//! blocks. Movement by visual line stays in the view, but which item it
//! moves into is decided here ([`Editor::vertical_neighbor`]).

use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use crate::blocks::BlockKind;
use crate::document::{DiskChange, DiskChangeKind, Document, SaveError};
use crate::editing;
use crate::history::{EditKind, History};
use crate::model::{Gap, Model, Splice};
use crate::movement;
pub use crate::movement::Selection;

/// What an edit changed: which of the model's items to re-measure, and the
/// selection afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub splice: Splice,
    pub selection: Selection,
}

/// What reloading changed: which items to re-measure, if the text
/// changed, and the selection afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reload {
    pub splice: Option<Splice>,
    pub selection: Option<Selection>,
}

/// A movement that doesn't depend on layout, forward or backward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// One character (grapheme).
    Grapheme,
    /// To the end of the next word, or the start of the previous one.
    Word,
    /// To the end or start of the document.
    Document,
}

impl Motion {
    fn target(self, text: &str, offset: usize, forward: bool) -> usize {
        match (self, forward) {
            (Motion::Grapheme, true) => movement::next_grapheme(text, offset),
            (Motion::Grapheme, false) => movement::prev_grapheme(text, offset),
            (Motion::Word, true) => movement::next_word_end(text, offset),
            (Motion::Word, false) => movement::prev_word_start(text, offset),
            (Motion::Document, true) => text.len(),
            (Motion::Document, false) => 0,
        }
    }
}

/// An item to show as source while the selection touches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealTarget {
    pub index: usize,
    /// The source shown: the item's source, or an empty range at the caret
    /// in a gap.
    pub range: Range<usize>,
    /// Set when the caret is on a blank line next to the item's block,
    /// which then stays rendered.
    pub gap: Option<Gap>,
    /// Whether this item holds the caret. It takes typing and shows the
    /// hint; other revealed items are only part of the selection.
    pub head: bool,
    /// How to style the source: the item's kind, or a paragraph in a gap.
    pub kind: BlockKind,
}

/// A document being edited.
#[derive(Debug)]
pub struct Editor {
    document: Document,
    model: Rc<Model>,
    history: History,
    /// The selection, or `None` before the first click or key press and
    /// after editing finishes.
    selection: Option<Selection>,
    /// Text being composed with an input method.
    marked: Option<Range<usize>>,
    /// View-only (e.g. a built-in document): nothing can be selected or
    /// changed.
    read_only: bool,
    /// Whether the text differs from the saved text, kept up to date after
    /// each change rather than compared on every frame.
    dirty: bool,
}

impl Editor {
    pub fn new(document: Document, read_only: bool) -> Self {
        let model = Model::new(&document.buffer().rope().to_string());
        let dirty = document.is_dirty();
        Self {
            document,
            model: Rc::new(model),
            history: History::default(),
            selection: None,
            marked: None,
            read_only,
            dirty,
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn model(&self) -> &Rc<Model> {
        &self.model
    }

    pub fn text(&self) -> &str {
        &self.model.text
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// The text being composed with an input method, if any.
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// The caret's offset, or the document start if there's no caret yet.
    pub fn caret(&self) -> usize {
        self.selection.map_or(0, |selection| selection.head)
    }

    // Files

    /// Saves to the document's file. See [`Document::save`].
    pub fn save(&mut self) -> Result<(), SaveError> {
        let result = self.document.save();
        self.dirty = self.document.is_dirty();
        result
    }

    /// Saves to `path`. See [`Document::save_as`].
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), SaveError> {
        let result = self.document.save_as(path);
        self.dirty = self.document.is_dirty();
        result
    }

    /// Accepts a change another app made to the file without reloading
    /// ("Keep Mine"). See [`Document::accept_disk_change`].
    pub fn accept_disk_change(&mut self, change: &DiskChange) {
        self.document.accept_disk_change(change);
        self.dirty = self.document.is_dirty();
    }

    /// Replaces the text with the file's new text after another app
    /// changed it, as one undo step, so it can be undone. Only the part
    /// that differs is replaced, and the selection keeps its place in the
    /// text around it; if it was inside that part, it moves to its start.
    /// Returns `None` if `change` has no new text, or it's the same text.
    pub fn reload(&mut self, change: &DiskChange) -> Option<Reload> {
        let DiskChangeKind::Changed(new) = &change.kind else {
            return None;
        };
        if self.read_only {
            return None;
        }
        let old = self.text();
        let range = differing_range(old, new);
        let inserted = range.start..new.len() - (old.len() - range.end);
        let map = |offset: usize| {
            if offset <= range.start {
                offset
            } else if offset >= range.end {
                offset - range.end + inserted.end
            } else {
                range.start
            }
        };
        let selection = self.selection.map(|selection| Selection {
            anchor: map(selection.anchor),
            head: map(selection.head),
        });
        let mut splice = None;
        if old != new {
            let before = self.selection.unwrap_or(Selection::caret(range.start));
            self.history.break_group();
            self.history.replace(
                self.document.buffer_mut(),
                range.clone(),
                &new[inserted],
                EditKind::Other,
                before,
                Instant::now(),
            );
            let after = selection.unwrap_or(Selection::caret(range.start));
            self.history.set_selection_after(after);
            splice = Some(self.text_changed(after).splice);
        }
        self.marked = None;
        self.selection = selection;
        self.document.accept_disk_change(change);
        self.dirty = self.document.is_dirty();
        Some(Reload { splice, selection })
    }

    /// Forgets that there are unsaved changes, once the user has chosen not
    /// to save them, so closing doesn't ask again.
    pub fn discard_changes(&mut self) {
        self.dirty = false;
    }

    // Selection

    /// Sets the selection. Returns false, changing nothing, if the document
    /// is read-only.
    pub fn select(&mut self, selection: Selection) -> bool {
        if self.read_only {
            return false;
        }
        self.selection = Some(selection);
        true
    }

    /// The selection after moving the caret to `offset`, extending the
    /// current selection if `extend` is set.
    pub fn selection_to(&self, offset: usize, extend: bool) -> Selection {
        let mut selection = self.selection.unwrap_or(Selection::caret(offset));
        selection.move_to(offset, extend);
        selection
    }

    /// The whole document, selected.
    pub fn all(&self) -> Selection {
        Selection {
            anchor: 0,
            head: self.text().len(),
        }
    }

    /// Removes the selection and any marked text: editing has finished.
    pub fn deselect(&mut self) {
        self.selection = None;
        self.marked = None;
        self.history.break_group();
    }

    /// Ends the current undo group, e.g. when the caret moves.
    pub fn break_group(&mut self) {
        self.history.break_group();
    }

    /// Ends input-method composition, keeping the composed text.
    pub fn unmark(&mut self) {
        self.marked = None;
    }

    /// Where `motion` takes the caret. Without `extend`, a selection
    /// collapses to its edge in the motion's direction instead (as on
    /// macOS), and the first key press just places the caret.
    pub fn motion_target(&self, motion: Motion, forward: bool, extend: bool) -> usize {
        let caret = self.caret();
        match self.selection {
            None => caret,
            Some(selection) if !extend && !selection.is_empty() => {
                let range = selection.range();
                if forward { range.end } else { range.start }
            }
            Some(_) => motion.target(self.text(), caret, forward),
        }
    }

    /// Where Up or Down goes from the caret's item's first or last line:
    /// the gap next to its block, the block next to its gap, or the
    /// neighboring item. `None` at the start or end of the document.
    pub fn vertical_neighbor(&self, down: bool) -> Option<usize> {
        let model = &self.model;
        let caret = self.caret();
        let index = model.item_at(caret);
        let item = &model.items[index];
        let source = model.source_range(index);
        let gap = self
            .selection
            .filter(Selection::is_empty)
            .and_then(|_| model.gap_at(index, caret));
        match (gap, down) {
            (Some(Gap::Above), true) => Some(source.start),
            (Some(Gap::Below), false) => Some(source.end),
            (None, true) if source.end < item.range.end => {
                Some(movement::next_grapheme(&model.text, source.end))
            }
            (None, false) if source.start > item.range.start => {
                Some(movement::prev_grapheme(&model.text, source.start))
            }
            (_, true) => model.items.get(index + 1).map(|item| item.range.start),
            (_, false) => index
                .checked_sub(1)
                .map(|index| model.items[index].range.end),
        }
    }

    /// Where ⌥↑ or ⌥↓ puts the caret: the start of the previous or next
    /// block's text, or of the block next to the caret's gap. Without a
    /// caret, the start of `first_visible`'s text.
    pub fn block_target(&self, down: bool, first_visible: usize) -> Option<usize> {
        let model = &self.model;
        let target = match self.selection {
            None => Some(first_visible),
            Some(selection) => {
                let index = model.item_at(selection.head);
                match (model.gap_at(index, selection.head), down) {
                    (Some(Gap::Above), true) | (Some(Gap::Below), false) => Some(index),
                    (_, true) => Some(index + 1).filter(|&next| next < model.items.len()),
                    (_, false) => index.checked_sub(1),
                }
            }
        };
        let item = model.items.get(target?)?;
        Some(if item.block.is_empty() {
            item.range.start
        } else {
            editing::content_start(&model.text, item.block.clone(), item.kind)
        })
    }

    /// The items to show as source: every item the selection touches, in
    /// order, or none without a selection. With an empty selection in a
    /// gap next to a block, the block stays rendered and the caret gets an
    /// empty line of its own.
    pub fn reveal_targets(&self) -> Vec<RevealTarget> {
        let Some(selection) = self.selection else {
            return Vec::new();
        };
        let model = &self.model;
        let head = selection.head;
        let head_index = model.item_at(head);
        let range = selection.range();
        let first = model.item_at(range.start);
        let mut last = model.item_at(range.end);
        // A selection that ends where the next item starts doesn't touch
        // it.
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
                let range = match gap {
                    Some(_) => head..head,
                    // A selection can reach into the blank lines beyond the
                    // source; then show the whole item.
                    None if is_head && !(source.start..=source.end).contains(&head) => {
                        model.items[index].range.clone()
                    }
                    None => source,
                };
                RevealTarget {
                    index,
                    range,
                    gap,
                    head: is_head,
                    kind: match gap {
                        Some(_) => BlockKind::Paragraph,
                        None => model.items[index].kind,
                    },
                }
            })
            .collect()
    }

    // Editing

    /// Replaces `range` with `text` and records it for undo. The caret ends
    /// after the inserted text.
    fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind) -> Option<Change> {
        if self.read_only {
            return None;
        }
        let before = self.selection.unwrap_or(Selection::caret(range.start));
        let selection = self.history.replace(
            self.document.buffer_mut(),
            range,
            text,
            kind,
            before,
            Instant::now(),
        );
        self.marked = None;
        Some(self.text_changed(selection))
    }

    /// Rebuilds the model after the buffer changed and selects `selection`.
    fn text_changed(&mut self, selection: Selection) -> Change {
        let text = self.document.buffer().rope().to_string();
        let (model, splice) = self.model.updated(&text);
        self.model = Rc::new(model);
        self.dirty = self.document.is_dirty();
        self.selection = Some(selection);
        Change { splice, selection }
    }

    /// Ticks or unticks the task list checkbox whose `[ ]` or `[x]` starts
    /// at `marker`, as one undo step. The selection stays as it is, so a
    /// click on a checkbox doesn't reveal its list. Returns the items to
    /// re-measure, or `None` if there's no checkbox there or the document
    /// is read-only.
    pub fn toggle_task(&mut self, marker: usize) -> Option<Splice> {
        if self.read_only {
            return None;
        }
        let mark = match self.text().get(marker..marker + 3)? {
            "[ ]" => "x",
            "[x]" | "[X]" => " ",
            _ => return None,
        };
        let selection = self.selection;
        let before = selection.unwrap_or(Selection::caret(marker));
        self.history.break_group();
        let after = self.history.replace(
            self.document.buffer_mut(),
            marker + 1..marker + 2,
            mark,
            EditKind::Other,
            before,
            Instant::now(),
        );
        self.history.break_group();
        let change = self.text_changed(after);
        // Redo restores the selection as it was too.
        self.history.set_selection_after(before);
        self.selection = selection;
        self.marked = None;
        Some(change.splice)
    }

    /// Replaces the selection with `text`.
    pub fn insert(&mut self, text: &str) -> Option<Change> {
        let range = self.selection?.range();
        self.replace(range, text, EditKind::Other)
    }

    /// Deletes the selection, or else from the caret to where `motion`
    /// takes it.
    pub fn delete(&mut self, motion: Motion, forward: bool) -> Option<Change> {
        let caret = self.selection?.head;
        self.delete_to(motion.target(self.text(), caret, forward))
    }

    /// ⌘⌫: deletes the selection, or else back to `line_start`, the start
    /// of the caret's visual line. At the line start, deletes the line
    /// break before it.
    pub fn delete_to_line_start(&mut self, line_start: usize) -> Option<Change> {
        let caret = self.selection?.head;
        if line_start == caret {
            self.delete_to(movement::prev_grapheme(self.text(), caret))
        } else {
            self.delete_to(line_start)
        }
    }

    /// Deletes the selection, or else from the caret to `other`.
    fn delete_to(&mut self, other: usize) -> Option<Change> {
        let range = self.selection?.range();
        let (range, kind) = if range.is_empty() {
            (
                range.start.min(other)..range.start.max(other),
                EditKind::Deleting,
            )
        } else {
            (range, EditKind::Other)
        };
        if range.is_empty() {
            return None;
        }
        self.replace(range, "", kind)
    }

    /// Enter, or Shift-Enter if `shift`: a line break that continues lists
    /// and quotes. See [`editing::enter`].
    pub fn newline(&mut self, shift: bool) -> Option<Change> {
        let range = self.selection?.range();
        let model = &self.model;
        let index = model.item_at(range.start);
        // In a gap, Enter just adds blank lines.
        let kind = match model.gap_at(index, range.start) {
            Some(_) => BlockKind::Paragraph,
            None => model.items[index].kind,
        };
        let line_ending = editing::line_ending(&model.text);
        let (range, text) = editing::enter(&model.text, kind, range, shift, line_ending);
        self.replace(range, &text, EditKind::Other)
    }

    /// Tab, Shift-Tab, ⌘] or ⌘[: indents list items and code. See
    /// [`editing::indent`]. The same text stays selected.
    pub fn indent(&mut self, outdent: bool, linewise: bool) -> Option<Change> {
        let selection = self.selection?;
        let model = &self.model;
        let index = model.item_at(selection.range().start);
        if model.gap_at(index, selection.range().start).is_some() {
            return None;
        }
        let item = &model.items[index];
        let edit = editing::indent(
            &model.text,
            item.kind,
            item.block.clone(),
            selection,
            outdent,
            linewise,
        )?;
        let mut change = self.replace(edit.range, &edit.text, EditKind::Other)?;
        self.history.set_selection_after(edit.selection);
        self.selection = Some(edit.selection);
        change.selection = edit.selection;
        Some(change)
    }

    /// The selected text, if anything is selected.
    pub fn selected_text(&self) -> Option<&str> {
        let range = self.selection?.range();
        (!range.is_empty()).then(|| &self.text()[range])
    }

    /// Deletes the selected text, if anything is selected, after the
    /// caller has copied it.
    pub fn cut(&mut self) -> Option<Change> {
        let range = self.selection?.range();
        if range.is_empty() {
            return None;
        }
        self.replace(range, "", EditKind::Other)
    }

    /// Replaces the selection with pasted `text`, converting its line
    /// endings to the document's.
    pub fn paste(&mut self, text: &str) -> Option<Change> {
        let text = editing::normalize_line_endings(text, editing::line_ending(self.text()));
        self.insert(&text)
    }

    pub fn undo(&mut self) -> Option<Change> {
        if self.read_only {
            return None;
        }
        let selection = self.history.undo(self.document.buffer_mut())?;
        self.marked = None;
        Some(self.text_changed(selection))
    }

    pub fn redo(&mut self) -> Option<Change> {
        if self.read_only {
            return None;
        }
        let selection = self.history.redo(self.document.buffer_mut())?;
        self.marked = None;
        Some(self.text_changed(selection))
    }

    // The platform's text input, which counts in UTF-16 code units.

    pub fn to_utf16(&self, offset: usize) -> usize {
        self.document.buffer().byte_to_utf16(offset)
    }

    pub fn from_utf16(&self, offset: usize) -> usize {
        self.document.buffer().utf16_to_byte(offset)
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.to_utf16(range.start)..self.to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.from_utf16(range.start)..self.from_utf16(range.end)
    }

    /// The range an input method edits: `range_utf16` if given, otherwise
    /// the marked text, otherwise the selection.
    fn input_range(&self, range_utf16: Option<Range<usize>>) -> Option<Range<usize>> {
        range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or(self.marked.clone())
            .or(self.selection.map(|selection| selection.range()))
    }

    /// Text typed or committed by an input method. Typing a character with
    /// nothing selected groups with the typing around it for undo.
    pub fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
    ) -> Option<Change> {
        let range = self.input_range(range_utf16)?;
        let kind = if range.is_empty() && self.marked.is_none() && !text.contains(['\n', '\r']) {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        self.replace(range, text, kind)
    }

    /// Text being composed by an input method, which stays marked.
    /// `selected_utf16` is the selection within `text`, if the input method
    /// gives one; otherwise the caret ends after it.
    pub fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> Option<Change> {
        let range = self.input_range(range_utf16)?;
        let mut change = self.replace(range.clone(), text, EditKind::Other)?;
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        if let Some(selected) = selected_utf16 {
            let base = self.to_utf16(range.start);
            let selection = Selection {
                anchor: self.from_utf16(base + selected.start),
                head: self.from_utf16(base + selected.end),
            };
            self.selection = Some(selection);
            change.selection = selection;
        }
        Some(change)
    }
}

/// The range of `old` that differs from `new`: everything between their
/// common start and common end, on character boundaries in both.
fn differing_range(old: &str, new: &str) -> Range<usize> {
    let boundary = |at: usize| old.is_char_boundary(at) && new.is_char_boundary(at);
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !boundary(start) {
        start -= 1;
    }
    let longest_end = old.len().min(new.len()) - start;
    let mut end = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(longest_end)
        .take_while(|(a, b)| a == b)
        .count();
    while !(old.is_char_boundary(old.len() - end) && new.is_char_boundary(new.len() - end)) {
        end -= 1;
    }
    start..old.len() - end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(text: &str) -> Editor {
        Editor::new(Document::from_text(text), false)
    }

    /// An editor on `text` with the caret at its `|`, or the selection
    /// from its `[` to its `|` (either order).
    fn editor_at(marked: &str) -> Editor {
        let text = marked.replace(['|', '['], "");
        let head = marked.find('|').unwrap();
        let anchor = marked.find('[');
        let mut editor = open(&text);
        let selection = match anchor {
            None => Selection::caret(head),
            Some(anchor) if anchor < head => Selection {
                anchor,
                head: head - 1,
            },
            Some(anchor) => Selection {
                anchor: anchor - 1,
                head,
            },
        };
        assert!(editor.select(selection));
        editor
    }

    /// The revealed items as (index, shown source, gap, head).
    fn revealed(editor: &Editor) -> Vec<(usize, &str, Option<Gap>, bool)> {
        editor
            .reveal_targets()
            .into_iter()
            .map(|target| {
                (
                    target.index,
                    &editor.text()[target.range],
                    target.gap,
                    target.head,
                )
            })
            .collect()
    }

    #[test]
    fn caret_reveals_its_item() {
        let editor = editor_at("One\n\nTw|o\n");
        assert_eq!(revealed(&editor), [(1, "Two\n", None, true)]);
        assert!(open("One\n").reveal_targets().is_empty());
    }

    #[test]
    fn selection_ending_at_next_item_start_does_not_reveal_it() {
        // Selecting all of "One" and its blank line, up to where "Two"
        // starts, with the caret at the start.
        let editor = editor_at("|One\n\n[Two\n");
        assert_eq!(revealed(&editor), [(0, "One\n", None, true)]);
        // With the caret there, it does.
        let editor = editor_at("[One\n\n|Two\n");
        assert_eq!(
            revealed(&editor),
            [(0, "One\n", None, false), (1, "Two\n", None, true)]
        );
    }

    #[test]
    fn selection_into_trailing_blank_lines_reveals_whole_item() {
        // The caret on the second blank line after "One", beyond its
        // source, with a selection: the whole item is shown.
        let editor = editor_at("[One\n\n|\n\nTwo\n");
        assert_eq!(revealed(&editor), [(0, "One\n\n\n", None, true)]);
    }

    #[test]
    fn empty_selection_in_gap_shows_empty_line() {
        let editor = editor_at("One\n\n|\nTwo\n");
        assert_eq!(revealed(&editor), [(0, "", Some(Gap::Below), true)]);
        let target = &editor.reveal_targets()[0];
        assert_eq!(target.range, 5..5);
        assert_eq!(target.kind, BlockKind::Paragraph);
    }

    #[test]
    fn up_and_down_neighbors_at_item_edges() {
        let text = "One\n\n\nTwo\n";
        // From a block: down into the gap below its source line, up into
        // the previous item.
        let editor = editor_at("One\n\n\nT|wo\n");
        assert_eq!(editor.vertical_neighbor(true), None);
        // The end of the previous item, which stops before the line
        // ending just above this block.
        assert_eq!(editor.vertical_neighbor(false), Some(5));
        let editor = editor_at("O|ne\n\n\nTwo\n");
        assert_eq!(editor.vertical_neighbor(true), Some(5));
        assert_eq!(editor.vertical_neighbor(false), None);
        // From the gap: up into its block's source, down into the next
        // item.
        let editor = editor_at("One\n\n|\nTwo\n");
        assert_eq!(editor.vertical_neighbor(false), Some(4));
        assert_eq!(
            editor.vertical_neighbor(true),
            Some(text.find("Two").unwrap())
        );
    }

    #[test]
    fn move_to_block_from_gap() {
        let text = "# One\n\n\n- Two\n";
        let two = text.find("Two").unwrap();
        // From the gap below "One": up goes back to "One"'s text, down to
        // the next item's text.
        let editor = editor_at("# One\n\n|\n- Two\n");
        assert_eq!(editor.block_target(false, 0), Some(2));
        assert_eq!(editor.block_target(true, 0), Some(two));
        // Without a caret: the first item on screen.
        assert_eq!(open(text).block_target(true, 1), Some(two));
    }

    #[test]
    fn motion_collapses_selection_or_moves() {
        let editor = editor_at("a[bc|d");
        assert_eq!(editor.motion_target(Motion::Grapheme, false, false), 1);
        assert_eq!(editor.motion_target(Motion::Grapheme, true, false), 3);
        assert_eq!(editor.motion_target(Motion::Grapheme, true, true), 4);
        assert_eq!(editor.motion_target(Motion::Document, false, true), 0);
        // The first key press only places the caret.
        assert_eq!(open("abc").motion_target(Motion::Word, true, false), 0);
    }

    #[test]
    fn delete_to_line_start_at_line_start_joins_lines() {
        let mut editor = editor_at("one\n|two");
        let change = editor.delete_to_line_start(4).unwrap();
        assert_eq!(editor.text(), "onetwo");
        assert_eq!(change.selection, Selection::caret(3));
        let mut editor = editor_at("one\ntw|o");
        editor.delete_to_line_start(4).unwrap();
        assert_eq!(editor.text(), "one\no");
    }

    #[test]
    fn edits_report_the_splice_and_selection() {
        let mut editor = editor_at("One\n\nTwo|\n");
        let change = editor.replace_text_in_range(None, "!").unwrap();
        assert_eq!(editor.text(), "One\n\nTwo!\n");
        assert_eq!(change.selection, Selection::caret(9));
        assert_eq!(change.splice.old, 1..2);
        assert_eq!(change.splice.new_count, 1);
        assert_eq!(editor.selection(), Some(change.selection));
    }

    #[test]
    fn indent_keeps_text_selected_through_redo() {
        let mut editor = editor_at("- a\n- b|\n");
        let change = editor.indent(false, false).unwrap();
        assert_eq!(editor.text(), "- a\n  - b\n");
        assert_eq!(change.selection, Selection::caret(9));
        editor.undo().unwrap();
        assert_eq!(editor.redo().unwrap().selection, Selection::caret(9));
    }

    #[test]
    fn read_only_refuses_selection_and_edits() {
        let mut editor = Editor::new(Document::from_text("Help\n"), true);
        assert!(!editor.select(Selection::caret(0)));
        assert_eq!(editor.selection(), None);
        assert!(editor.reveal_targets().is_empty());
        assert!(editor.insert("x").is_none());
        assert!(editor.replace_text_in_range(Some(0..0), "x").is_none());
        assert!(
            editor
                .replace_and_mark_text_in_range(Some(0..0), "x", None)
                .is_none()
        );
        assert!(editor.undo().is_none());
        assert_eq!(editor.text(), "Help\n");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn toggle_task_flips_the_checkbox_as_one_undo_step() {
        let mut editor = open("- [ ] a\n- [X] b\n\nText\n");
        assert!(editor.select(Selection::caret(18)));
        let splice = editor.toggle_task(2).unwrap();
        assert_eq!(editor.text(), "- [x] a\n- [X] b\n\nText\n");
        assert_eq!(splice.old, 0..1);
        assert_eq!(editor.selection(), Some(Selection::caret(18)));
        assert!(editor.is_dirty());
        editor.toggle_task(10).unwrap();
        assert_eq!(editor.text(), "- [x] a\n- [ ] b\n\nText\n");

        editor.undo().unwrap();
        assert_eq!(editor.text(), "- [x] a\n- [X] b\n\nText\n");
        editor.undo().unwrap();
        assert_eq!(editor.text(), "- [ ] a\n- [X] b\n\nText\n");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn toggle_task_without_a_selection_keeps_none() {
        let mut editor = open("- [x] a\n");
        editor.toggle_task(2).unwrap();
        assert_eq!(editor.text(), "- [ ] a\n");
        assert_eq!(editor.selection(), None);
    }

    #[test]
    fn toggle_task_needs_a_checkbox() {
        let mut editor = open("- [ ] a\n");
        assert!(editor.toggle_task(0).is_none());
        assert!(editor.toggle_task(7).is_none());
        assert!(editor.toggle_task(99).is_none());
        assert_eq!(editor.text(), "- [ ] a\n");
        let mut builtin = Editor::new(Document::from_text("- [ ] a\n"), true);
        assert!(builtin.toggle_task(2).is_none());
        assert_eq!(builtin.text(), "- [ ] a\n");
    }

    #[test]
    fn dirty_flag_follows_edits_and_undo() {
        let mut editor = editor_at("a|");
        assert!(!editor.is_dirty());
        editor.insert("b").unwrap();
        assert!(editor.is_dirty());
        editor.undo().unwrap();
        assert!(!editor.is_dirty());
        editor.redo().unwrap();
        assert!(editor.is_dirty());
        editor.discard_changes();
        assert!(!editor.is_dirty());
    }

    #[test]
    fn paste_uses_document_line_endings() {
        let mut editor = editor_at("a\r\n|");
        editor.paste("b\nc").unwrap();
        assert_eq!(editor.text(), "a\r\nb\r\nc");
    }

    #[test]
    fn selected_text_and_cut() {
        let mut editor = editor_at("a[bc|d");
        assert_eq!(editor.selected_text(), Some("bc"));
        editor.cut().unwrap();
        assert_eq!(editor.text(), "ad");
        assert_eq!(editor.selected_text(), None);
        assert!(editor.cut().is_none());
    }

    #[test]
    fn ime_compose_replace_and_undo() {
        // Composing Korean "한" after an emoji: the input method marks
        // "ㅎ", replaces it with "하", then commits "한". Offsets from the
        // input method are UTF-16; the emoji is two units.
        let mut editor = editor_at("😀|");
        let change = editor
            .replace_and_mark_text_in_range(None, "ㅎ", Some(1..1))
            .unwrap();
        assert_eq!(editor.text(), "😀ㅎ");
        assert_eq!(editor.marked(), Some(4..7));
        assert_eq!(change.selection, Selection::caret(7));
        assert_eq!(editor.range_to_utf16(&(4..7)), 2..3);

        editor.replace_and_mark_text_in_range(None, "하", Some(1..1));
        assert_eq!(editor.text(), "😀하");
        assert_eq!(editor.marked(), Some(4..7));

        let change = editor.replace_text_in_range(None, "한").unwrap();
        assert_eq!(editor.text(), "😀한");
        assert_eq!(editor.marked(), None);
        assert_eq!(change.selection, Selection::caret(7));

        // Each step is its own undo step, back to the original text.
        editor.undo().unwrap();
        assert_eq!(editor.text(), "😀하");
        editor.undo().unwrap();
        assert_eq!(editor.text(), "😀ㅎ");
        let change = editor.undo().unwrap();
        assert_eq!(editor.text(), "😀");
        assert_eq!(change.selection, Selection::caret(4));
        assert_eq!(editor.marked(), None);
    }

    #[test]
    fn ime_selection_within_marked_text() {
        // The input method selects the first of two composed characters.
        let mut editor = editor_at("ab|");
        let change = editor
            .replace_and_mark_text_in_range(Some(1..2), "éé", Some(0..1))
            .unwrap();
        assert_eq!(editor.text(), "aéé");
        assert_eq!(editor.marked(), Some(1..5));
        assert_eq!(change.selection, Selection { anchor: 1, head: 3 });
    }

    #[test]
    fn typing_groups_for_undo() {
        let mut editor = editor_at("|");
        for text in ["a", "b", "c"] {
            editor.replace_text_in_range(None, text).unwrap();
        }
        editor.newline(false).unwrap();
        assert_eq!(editor.text(), "abc\n");
        editor.undo().unwrap();
        assert_eq!(editor.undo().unwrap().selection, Selection::caret(0));
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn differing_range_is_on_character_boundaries() {
        assert_eq!(differing_range("abc", "abc"), 3..3);
        assert_eq!(differing_range("a1c", "a22c"), 1..2);
        assert_eq!(differing_range("ab", "abab"), 2..2);
        assert_eq!(differing_range("abab", "ab"), 2..4);
        // é and è share their first byte; ü and ä their last... neither
        // may be split.
        assert_eq!(differing_range("café", "cafè"), 3..5);
        assert_eq!(differing_range("xéy", "xèy"), 1..3);
        assert_eq!(differing_range("", "new"), 0..0);
    }

    /// An editor on a file holding `text`, and a change to `changed` made
    /// by another app.
    fn edited_elsewhere(name: &str, text: &str, changed: &str) -> (Editor, DiskChange) {
        let dir = std::env::temp_dir().join(format!("kami-editor-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.md");
        std::fs::write(&path, text).unwrap();
        let editor = Editor::new(Document::open(&path).unwrap(), false);
        std::fs::write(&path, changed).unwrap();
        let change = editor.document().disk_change().unwrap();
        std::fs::remove_dir_all(&dir).ok();
        (editor, change)
    }

    #[test]
    fn reload_is_one_undo_step_and_keeps_the_caret_in_place() {
        let (mut editor, change) = edited_elsewhere(
            "reload",
            "# One\n\nTwo\n\nThree\n",
            "# One!\n\nTwo\n\nThree\n",
        );
        editor.select(Selection::caret(14));
        let reload = editor.reload(&change).unwrap();
        assert_eq!(editor.text(), "# One!\n\nTwo\n\nThree\n");
        assert_eq!(
            reload.selection,
            Some(Selection::caret(15)),
            "after the change"
        );
        assert!(reload.splice.is_some());
        assert!(!editor.is_dirty());

        let undone = editor.undo().unwrap();
        assert_eq!(editor.text(), "# One\n\nTwo\n\nThree\n");
        assert_eq!(undone.selection, Selection::caret(14));
        assert!(editor.is_dirty(), "differs from the file again");
        editor.redo().unwrap();
        assert_eq!(editor.text(), "# One!\n\nTwo\n\nThree\n");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn reload_without_a_caret_keeps_none() {
        let (mut editor, change) = edited_elsewhere("no-caret", "a\n", "b\n");
        let reload = editor.reload(&change).unwrap();
        assert_eq!(reload.selection, None);
        assert_eq!(editor.selection(), None);
        assert_eq!(editor.text(), "b\n");
    }

    #[test]
    fn reload_moves_a_caret_inside_the_change_to_its_start() {
        let (mut editor, change) = edited_elsewhere("inside", "one two three", "one 2 three");
        editor.select(Selection::caret(6));
        let reload = editor.reload(&change).unwrap();
        assert_eq!(reload.selection, Some(Selection::caret(4)));
    }

    #[test]
    fn keeping_mine_marks_the_text_unsaved() {
        let (mut editor, change) = edited_elsewhere("keep", "mine\n", "theirs\n");
        assert!(!editor.is_dirty());
        editor.accept_disk_change(&change);
        assert!(editor.is_dirty());
        assert_eq!(editor.text(), "mine\n");
    }
}
