//! Ranges of the text the host marks (#120), and the menu it can show on
//! them.

use std::rc::Rc;

use gpui::{Context, Pixels, Point};

use super::{DocumentEvent, DocumentView};
use crate::marks::{self, Mark};

impl DocumentView {
    /// Marks ranges of the document's text, replacing the marks there were
    /// (see [`Mark`]). They move with the text as it's edited, and a mark
    /// whose text is edited goes, until the host marks the text again,
    /// say once [`DocumentEvent::Edited`] comes. A right-click or a
    /// Control-click on one sends [`DocumentEvent::MarkMenu`]. Opening
    /// another document clears them.
    pub fn set_marks(&mut self, mut marks: Vec<Mark>, cx: &mut Context<Self>) {
        marks.sort_by_key(|mark| (mark.range.start, mark.range.end));
        self.marks_model = self
            .open
            .as_ref()
            .filter(|_| !marks.is_empty())
            .map(|open| open.editor.model().clone());
        self.marks = Rc::new(marks);
        cx.notify();
    }

    /// The marks, where they are in the text now.
    pub fn marks(&mut self) -> &[Mark] {
        self.move_marks();
        &self.marks
    }

    /// Moves the marks to the text as it is now, if it's changed since
    /// they were last moved.
    pub(super) fn move_marks(&mut self) {
        let (Some(open), Some(old)) = (&self.open, &self.marks_model) else {
            return;
        };
        let new = open.editor.model();
        if Rc::ptr_eq(old, new) {
            return;
        }
        self.marks = Rc::new(marks::moved(&self.marks, &old.text, &new.text));
        self.marks_model = Some(new.clone());
    }

    /// Tells the host `mark` was right-clicked or Control-clicked at
    /// `position`.
    pub(crate) fn mark_menu(
        &mut self,
        mark: &Mark,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(DocumentEvent::MarkMenu {
            id: mark.id,
            range: mark.range.clone(),
            position,
        });
    }
}
