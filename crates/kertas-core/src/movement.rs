//! Caret movement and selection over a document's source.
//!
//! These are pure functions of the source text. Movement that depends on
//! layout (visual lines, Up/Down) lives in the view. Every offset can hold
//! the caret, including blank lines between blocks (see
//! [`crate::blocks::item_ranges`]).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// A selection in source byte offsets. The caret is at `head`; `anchor` is
/// where the selection started. They're equal when nothing is selected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    /// An empty selection with the caret at `offset`.
    pub fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    /// The selected range, in source order.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Moves the caret to `offset`, extending the selection if `extend` is
    /// set, or collapsing it otherwise.
    pub fn move_to(&mut self, offset: usize, extend: bool) {
        self.head = offset;
        if !extend {
            self.anchor = offset;
        }
    }
}

/// The offset of the next grapheme boundary after `offset`.
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    text[offset..]
        .grapheme_indices(true)
        .nth(1)
        .map_or(text.len(), |(index, _)| offset + index)
}

/// The offset of the previous grapheme boundary before `offset`.
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    text[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(index, _)| index)
}

/// The end of the next word after `offset`, skipping any whitespace and
/// punctuation before it. Returns `text.len()` if there's no word after.
pub fn next_word_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .split_word_bound_indices()
        .map(|(index, word)| (offset + index, word))
        .find(|(start, word)| is_word(word) && start + word.len() > offset)
        .map_or(text.len(), |(start, word)| start + word.len())
}

/// The start of the previous word before `offset`, skipping any whitespace
/// and punctuation after it. Returns 0 if there's no word before.
pub fn prev_word_start(text: &str, offset: usize) -> usize {
    text[..offset]
        .split_word_bound_indices()
        .rev()
        .find(|(_, word)| is_word(word))
        .map_or(0, |(start, _)| start)
}

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_range_and_extend() {
        let mut selection = Selection::caret(5);
        assert!(selection.is_empty());
        selection.move_to(2, true);
        assert_eq!(selection.range(), 2..5);
        selection.move_to(8, true);
        assert_eq!(selection.range(), 5..8);
        selection.move_to(3, false);
        assert_eq!(selection, Selection::caret(3));
    }

    #[test]
    fn graphemes() {
        let text = "a🦀e\u{301}\r\nb";
        // a | 🦀 (4 bytes) | e + combining accent (3 bytes) | \r\n | b
        assert_eq!(next_grapheme(text, 0), 1);
        assert_eq!(next_grapheme(text, 1), 5);
        assert_eq!(next_grapheme(text, 5), 8);
        assert_eq!(next_grapheme(text, 8), 10, "CRLF is one grapheme");
        assert_eq!(next_grapheme(text, 11), 11);
        assert_eq!(prev_grapheme(text, 10), 8);
        assert_eq!(prev_grapheme(text, 8), 5);
        assert_eq!(prev_grapheme(text, 5), 1);
        assert_eq!(prev_grapheme(text, 0), 0);
    }

    #[test]
    fn words() {
        let text = "Hello, **big** world!";
        assert_eq!(next_word_end(text, 0), 5);
        assert_eq!(next_word_end(text, 5), 12, "skips punctuation to `big`");
        assert_eq!(next_word_end(text, 12), 20);
        assert_eq!(next_word_end(text, 20), text.len());
        assert_eq!(prev_word_start(text, text.len()), 15);
        assert_eq!(prev_word_start(text, 15), 9);
        assert_eq!(prev_word_start(text, 9), 0);
        assert_eq!(prev_word_start(text, 3), 0);
    }

    #[test]
    fn words_non_ascii() {
        let text = "café naïve";
        assert_eq!(next_word_end(text, 0), "café".len());
        assert_eq!(prev_word_start(text, text.len()), "café ".len());
    }

    #[test]
    fn crossing_blank_lines_by_word() {
        let text = "end.\n\nNext";
        assert_eq!(next_word_end(text, 3), 10, "skips the blank line to `Next`");
        assert_eq!(prev_word_start(text, 6), 0);
    }
}
