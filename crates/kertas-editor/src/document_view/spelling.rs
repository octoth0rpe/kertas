//! Underlining misspelled words (#141), checked in the background once
//! typing pauses, in the language the configuration names.

use std::rc::Rc;

use gpui::{Context, Hsla};
use kertas_core::rendered::AlertKind;

use super::DocumentView;
use crate::marks::{self, Mark, MarkStyle};

/// The misspelled words, as marks of their own, separate from the
/// host's, drawn with a wavy underline in the theme's caution color.
#[derive(Default)]
pub(super) struct Misspelled {
    pub(super) marks: Rc<Vec<Mark>>,
    /// The text the marks were last moved to, as the host's marks have.
    model: Option<Rc<kertas_core::model::Model>>,
    /// Bumped to drop a check the text has moved on from.
    epoch: usize,
}

impl DocumentView {
    /// Checks the document's spelling, in the background, and underlines
    /// what's misspelled once that's done. Spelling isn't checked if the
    /// configuration turns it off, there's no dictionary for its
    /// language, or the document can't be edited.
    pub(super) fn check_spelling(&mut self, cx: &mut Context<Self>) {
        self.misspelled.epoch += 1;
        let epoch = self.misspelled.epoch;
        let model = self
            .open
            .as_ref()
            .filter(|open| {
                self.config.check_spelling && !open.editor.read_only() && self.text_name.is_none()
            })
            .map(|open| open.editor.model().clone());
        let loading = model
            .as_ref()
            .and_then(|_| crate::spelling::checker(&self.config.spelling_language, cx));
        let (Some(model), Some(loading)) = (model, loading) else {
            if !self.misspelled.marks.is_empty() {
                self.misspelled = Misspelled {
                    epoch,
                    ..Misspelled::default()
                };
                cx.notify();
            }
            return;
        };
        let syntax = self.config.syntax();
        let text = model.text.to_string();
        cx.spawn(async move |this, cx| {
            let Some(checker) = loading.await else {
                return;
            };
            let found = cx
                .background_executor()
                .spawn(async move { checker.misspelled(&text, syntax) })
                .await;
            this.update(cx, |view, cx| {
                if view.misspelled.epoch != epoch {
                    return;
                }
                let color = view.misspelling_color();
                let marks = found
                    .into_iter()
                    .map(|range| Mark {
                        range,
                        style: MarkStyle::Wavy(color),
                        id: 0,
                    })
                    .collect();
                view.misspelled.marks = Rc::new(marks);
                view.misspelled.model = Some(model);
                // The text may have changed while it was being checked.
                view.move_misspellings();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The color misspelled words are underlined in.
    fn misspelling_color(&self) -> Hsla {
        self.theme().alert(AlertKind::Caution)
    }

    /// Moves the underlines to the text as it is now, dropping those whose
    /// words were edited, until the next check.
    pub(super) fn move_misspellings(&mut self) {
        let (Some(open), Some(old)) = (&self.open, &self.misspelled.model) else {
            return;
        };
        let new = open.editor.model();
        if Rc::ptr_eq(old, new) {
            return;
        }
        let moved = marks::moved(&self.misspelled.marks, &old.text, &new.text);
        self.misspelled.marks = Rc::new(moved);
        self.misspelled.model = Some(new.clone());
    }

    /// Moves the underlines to the text as it is now, in the theme's
    /// current color, before drawing.
    pub(super) fn refresh_misspellings(&mut self) {
        self.move_misspellings();
        let color = self.misspelling_color();
        let stale = self
            .misspelled
            .marks
            .first()
            .is_some_and(|mark| mark.style != MarkStyle::Wavy(color));
        if stale {
            let recolored = self
                .misspelled
                .marks
                .iter()
                .map(|mark| Mark {
                    style: MarkStyle::Wavy(color),
                    ..mark.clone()
                })
                .collect();
            self.misspelled.marks = Rc::new(recolored);
        }
    }

    /// The misspelled words' ranges in the text as it is now, for the
    /// smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn misspelled_ranges(&mut self) -> Vec<std::ops::Range<usize>> {
        self.move_misspellings();
        self.misspelled
            .marks
            .iter()
            .map(|mark| mark.range.clone())
            .collect()
    }
}
