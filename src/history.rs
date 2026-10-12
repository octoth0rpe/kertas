//! Undo and redo.
//!
//! Each edit is recorded as a change to the buffer. Consecutive typing, or
//! consecutive deleting, at adjacent positions is grouped into one undo
//! step, as in macOS text fields. A pause, a caret movement or a different
//! kind of edit starts a new step.

use std::ops::Range;
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::movement::Selection;

/// Edits closer together than this can be grouped.
const GROUP_INTERVAL: Duration = Duration::from_millis(1000);

/// What kind of edit a change is, for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Typing characters; groups with adjacent typing.
    Typing,
    /// Deleting with Backspace or Delete; groups with adjacent deletes.
    Deleting,
    /// Anything else (paste, cut, newline, IME); always its own step.
    Other,
}

#[derive(Debug, Clone)]
struct Change {
    /// Where the change starts.
    at: usize,
    old_text: String,
    new_text: String,
}

#[derive(Debug, Clone)]
struct Step {
    changes: Vec<Change>,
    selection_before: Selection,
    selection_after: Selection,
    kind: EditKind,
    last_edit: Instant,
}

/// The undo and redo stacks.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Whether the next edit may join the last undo step.
    can_group: bool,
}

impl History {
    /// Replaces `range` in `buffer` with `text` and records it.
    /// `selection_before` is restored by undo; the caret ends after the
    /// inserted text.
    pub fn replace(
        &mut self,
        buffer: &mut Buffer,
        range: Range<usize>,
        text: &str,
        kind: EditKind,
        selection_before: Selection,
        now: Instant,
    ) -> Selection {
        let change = Change {
            at: range.start,
            old_text: buffer.slice(range.clone()),
            new_text: text.to_owned(),
        };
        buffer.replace(range.clone(), text);
        let selection_after = Selection::caret(range.start + text.len());
        self.redo.clear();

        let joins = self.can_group
            && kind != EditKind::Other
            && self.undo.last().is_some_and(|step| {
                step.kind == kind
                    && now.duration_since(step.last_edit) < GROUP_INTERVAL
                    && step
                        .changes
                        .last()
                        .is_some_and(|last| adjacent(last, &change))
            });
        if joins && let Some(step) = self.undo.last_mut() {
            step.changes.push(change);
            step.selection_after = selection_after;
            step.last_edit = now;
        } else {
            self.undo.push(Step {
                changes: vec![change],
                selection_before,
                selection_after,
                kind,
                last_edit: now,
            });
        }
        self.can_group = kind != EditKind::Other;
        selection_after
    }

    /// Stops the next edit from joining the current undo step, e.g. after
    /// the caret moves.
    pub fn break_group(&mut self) {
        self.can_group = false;
    }

    /// Undoes the last step, returning the selection to restore.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let step = self.undo.pop()?;
        for change in step.changes.iter().rev() {
            buffer.replace(
                change.at..change.at + change.new_text.len(),
                &change.old_text,
            );
        }
        let selection = step.selection_before;
        self.redo.push(step);
        self.can_group = false;
        Some(selection)
    }

    /// Redoes the last undone step, returning the selection to restore.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let step = self.redo.pop()?;
        for change in &step.changes {
            buffer.replace(
                change.at..change.at + change.old_text.len(),
                &change.new_text,
            );
        }
        let selection = step.selection_after;
        self.undo.push(step);
        self.can_group = false;
        Some(selection)
    }
}

/// Whether `next` continues `last`: typing right after it, or deleting
/// right before it (Backspace) or at the same place (Delete).
fn adjacent(last: &Change, next: &Change) -> bool {
    let typed = next.old_text.is_empty() && next.at == last.at + last.new_text.len();
    let backspaced = next.new_text.is_empty() && next.at + next.old_text.len() == last.at;
    let deleted_forward = next.new_text.is_empty() && next.at == last.at;
    typed || backspaced || deleted_forward
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buffer: &Buffer) -> String {
        buffer.slice(0..buffer.len_bytes())
    }

    struct Fixture {
        buffer: Buffer,
        history: History,
        selection: Selection,
        now: Instant,
    }

    impl Fixture {
        fn new(text: &str) -> Self {
            Self {
                buffer: Buffer::from_bytes(text.as_bytes()).unwrap(),
                history: History::default(),
                selection: Selection::caret(0),
                now: Instant::now(),
            }
        }

        fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind) {
            self.now += Duration::from_millis(100);
            self.selection = self.history.replace(
                &mut self.buffer,
                range,
                text,
                kind,
                self.selection,
                self.now,
            );
        }

        fn type_text(&mut self, text: &str) {
            for ch in text.chars() {
                let at = self.selection.head;
                self.edit(at..at, &ch.to_string(), EditKind::Typing);
            }
        }

        fn backspace(&mut self) {
            let at = self.selection.head;
            self.edit(at - 1..at, "", EditKind::Deleting);
        }
    }

    #[test]
    fn typing_groups_into_one_undo_step() {
        let mut f = Fixture::new("");
        f.type_text("hello");
        assert_eq!(text(&f.buffer), "hello");
        assert_eq!(f.history.undo(&mut f.buffer), Some(Selection::caret(0)));
        assert_eq!(text(&f.buffer), "");
        assert_eq!(f.history.redo(&mut f.buffer), Some(Selection::caret(5)));
        assert_eq!(text(&f.buffer), "hello");
    }

    #[test]
    fn backspacing_groups_into_one_undo_step() {
        let mut f = Fixture::new("hello");
        f.selection = Selection::caret(5);
        f.backspace();
        f.backspace();
        assert_eq!(text(&f.buffer), "hel");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "hello");
    }

    #[test]
    fn switching_kinds_starts_a_new_step() {
        let mut f = Fixture::new("");
        f.type_text("abc");
        f.backspace();
        assert_eq!(text(&f.buffer), "ab");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "abc");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "");
    }

    #[test]
    fn break_group_starts_a_new_step() {
        let mut f = Fixture::new("");
        f.type_text("ab");
        f.history.break_group();
        f.type_text("cd");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "ab");
    }

    #[test]
    fn pause_starts_a_new_step() {
        let mut f = Fixture::new("");
        f.type_text("ab");
        f.now += Duration::from_secs(2);
        f.type_text("cd");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "ab");
    }

    #[test]
    fn other_edits_are_their_own_step() {
        let mut f = Fixture::new("");
        f.type_text("ab");
        f.edit(2..2, "\n", EditKind::Other);
        f.type_text("c");
        assert_eq!(text(&f.buffer), "ab\nc");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "ab\n");
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "ab");
    }

    #[test]
    fn replacing_a_selection_undoes_to_the_selection() {
        let mut f = Fixture::new("hello world");
        f.selection = Selection {
            anchor: 6,
            head: 11,
        };
        f.edit(6..11, "there", EditKind::Other);
        assert_eq!(text(&f.buffer), "hello there");
        assert_eq!(
            f.history.undo(&mut f.buffer),
            Some(Selection {
                anchor: 6,
                head: 11
            })
        );
        assert_eq!(text(&f.buffer), "hello world");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut f = Fixture::new("");
        f.type_text("a");
        f.selection = f.history.undo(&mut f.buffer).unwrap();
        f.type_text("b");
        assert_eq!(f.history.redo(&mut f.buffer), None);
        assert_eq!(text(&f.buffer), "b");
    }

    #[test]
    fn non_ascii_undo() {
        let mut f = Fixture::new("");
        f.type_text("café 🦀");
        f.backspace_char();
        assert_eq!(text(&f.buffer), "café ");
        f.history.undo(&mut f.buffer);
        f.history.undo(&mut f.buffer);
        assert_eq!(text(&f.buffer), "");
    }

    impl Fixture {
        fn backspace_char(&mut self) {
            let at = self.selection.head;
            let start = crate::movement::prev_grapheme(&text(&self.buffer), at);
            self.edit(start..at, "", EditKind::Deleting);
        }
    }
}
