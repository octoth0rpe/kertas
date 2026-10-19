//! Ranges of a document's text that the host marks (#120), such as
//! misspelled words or names it recognizes, and how they move with edits.

use std::ops::Range;

use gpui::Hsla;

/// A range of a document's text that the host marked, with
/// [`DocumentView::set_marks`](crate::DocumentView::set_marks).
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Byte offsets in the document's text.
    pub range: Range<usize>,
    pub style: MarkStyle,
    /// What the host calls the mark, given back with it in
    /// [`DocumentEvent::MarkMenu`](crate::DocumentEvent::MarkMenu) and
    /// [`HoverTarget::Mark`](crate::HoverTarget::Mark).
    pub id: usize,
}

/// How a mark is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkStyle {
    /// Not at all: the text looks as it would without the mark, which
    /// only matters to the host's menu and popover.
    Hidden,
    /// With a wavy underline in this color, as a misspelled word is.
    Wavy(Hsla),
}

/// The part of the text an edit changed: `old` in the text before it,
/// which became `new` after it.
#[derive(Debug, PartialEq, Eq)]
struct Changed {
    old: Range<usize>,
    new: Range<usize>,
}

/// What changed between `old` and `new`, found by trimming what they have in
/// common at each end. `None` if they're the same.
fn changed(old: &str, new: &str) -> Option<Changed> {
    if old == new {
        return None;
    }
    let (old_bytes, new_bytes) = (old.as_bytes(), new.as_bytes());
    let prefix = old_bytes
        .iter()
        .zip(new_bytes)
        .take_while(|(a, b)| a == b)
        .count();
    let longest_suffix = old.len().min(new.len()) - prefix;
    let suffix = old_bytes
        .iter()
        .rev()
        .zip(new_bytes.iter().rev())
        .take(longest_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    Some(Changed {
        old: prefix..old.len() - suffix,
        new: prefix..new.len() - suffix,
    })
}

/// `marks` moved from the text `old` to the text `new`: those after the
/// change move with the text, and those it touches are dropped, until the
/// host marks the text again. Text typed just before or after a mark isn't
/// part of it.
pub(crate) fn moved(marks: &[Mark], old: &str, new: &str) -> Vec<Mark> {
    let Some(changed) = changed(old, new) else {
        return marks.to_vec();
    };
    marks
        .iter()
        .filter_map(|mark| {
            let range = &mark.range;
            if range.end <= changed.old.start {
                Some(mark.clone())
            } else if range.start >= changed.old.end {
                let shift = |offset: usize| offset - changed.old.end + changed.new.end;
                Some(Mark {
                    range: shift(range.start)..shift(range.end),
                    ..mark.clone()
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(range: Range<usize>) -> Mark {
        Mark {
            range,
            style: MarkStyle::Hidden,
            id: 0,
        }
    }

    fn ranges(marks: &[Mark]) -> Vec<Range<usize>> {
        marks.iter().map(|mark| mark.range.clone()).collect()
    }

    #[test]
    fn finds_what_changed() {
        assert_eq!(changed("abc", "abc"), None);
        assert_eq!(
            changed("Mikey said", "Mikey has said"),
            Some(Changed {
                old: 6..6,
                new: 6..10
            })
        );
        assert_eq!(
            changed("aaa", "aa"),
            Some(Changed {
                old: 2..3,
                new: 2..2
            })
        );
        assert_eq!(
            changed("abcd", "aXd"),
            Some(Changed {
                old: 1..3,
                new: 1..2
            })
        );
    }

    #[test]
    fn marks_follow_the_text() {
        // "Ann met Bob." with both names marked.
        let marks = [mark(0..3), mark(8..11)];
        let moved = moved(&marks, "Ann met Bob.", "Ann then met Bob.");
        assert_eq!(ranges(&moved), [0..3, 13..16]);
    }

    #[test]
    fn edited_marks_are_dropped() {
        let marks = [mark(0..3), mark(8..11)];
        let moved = moved(&marks, "Ann met Bob.", "Ann met Rob.");
        assert_eq!(ranges(&moved), [0..3]);
    }

    #[test]
    fn typing_next_to_a_mark_leaves_it_alone() {
        let marks = [mark(4..7)];
        assert_eq!(ranges(&moved(&marks, "Hi, Bob.", "Hi, Bob's.")), [4..7]);
        assert_eq!(ranges(&moved(&marks, "Hi, Bob.", "Hi, ABob.")), [5..8]);
    }
}
