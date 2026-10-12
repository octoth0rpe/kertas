//! Caret movement and selection over a document's source.
//!
//! These are pure functions of the source text and its blocks. Movement that
//! depends on layout (visual lines, Up/Down) lives in the view.
//!
//! The caret only ever sits inside a block (see
//! `docs/planning/caret-and-reveal.md`): moving past a block's edge jumps
//! over the blank lines to the neighboring block.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::blocks::Blocks;

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

/// Moves one grapheme right, or to the start of the next block from the
/// end of a block.
pub fn move_right(text: &str, blocks: &Blocks, offset: usize) -> usize {
    match block_range(blocks, offset) {
        Some(range) if offset < range.end => next_grapheme(text, offset).min(range.end),
        _ => next_block_start(blocks, offset).unwrap_or(offset),
    }
}

/// Moves one grapheme left, or to the end of the previous block from the
/// start of a block.
pub fn move_left(text: &str, blocks: &Blocks, offset: usize) -> usize {
    match block_range(blocks, offset) {
        Some(range) if offset > range.start => prev_grapheme(text, offset).max(range.start),
        _ => prev_block_end(blocks, offset).unwrap_or(offset),
    }
}

/// Moves to the end of the next word in the block, or to the start of the
/// next block from the end of a block.
pub fn move_word_right(text: &str, blocks: &Blocks, offset: usize) -> usize {
    match block_range(blocks, offset) {
        Some(range) if offset < range.end => next_word_end(text, offset).min(range.end),
        _ => next_block_start(blocks, offset).unwrap_or(offset),
    }
}

/// Moves to the start of the previous word in the block, or to the end of
/// the previous block from the start of a block.
pub fn move_word_left(text: &str, blocks: &Blocks, offset: usize) -> usize {
    match block_range(blocks, offset) {
        Some(range) if offset > range.start => prev_word_start(text, offset).max(range.start),
        _ => prev_block_end(blocks, offset).unwrap_or(offset),
    }
}

/// The first caret position in the document.
pub fn document_start(blocks: &Blocks) -> usize {
    blocks
        .as_slice()
        .first()
        .map_or(0, |block| block.range.start)
}

/// The last caret position in the document.
pub fn document_end(blocks: &Blocks) -> usize {
    blocks.as_slice().last().map_or(0, |block| block.range.end)
}

/// The start of the first block after `offset`'s block (or after `offset`,
/// if it's between blocks).
pub fn next_block_start(blocks: &Blocks, offset: usize) -> Option<usize> {
    blocks
        .as_slice()
        .iter()
        .find(|block| block.range.start > offset)
        .map(|block| block.range.start)
}

/// The end of the last block before `offset`'s block (or before `offset`,
/// if it's between blocks).
pub fn prev_block_end(blocks: &Blocks, offset: usize) -> Option<usize> {
    blocks
        .as_slice()
        .iter()
        .rev()
        .find(|block| block.range.end < offset)
        .map(|block| block.range.end)
}

fn block_range(blocks: &Blocks, offset: usize) -> Option<Range<usize>> {
    blocks.at(offset).map(|block| block.range.clone())
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

    /// "# Head" is 0..6, "Para one" is 8..16, "- item" is 18..24.
    const DOC: &str = "# Head\n\nPara one\n\n- item\n";

    #[test]
    fn right_moves_within_block_then_jumps_gap() {
        let blocks = Blocks::parse(DOC);
        assert_eq!(move_right(DOC, &blocks, 0), 1);
        assert_eq!(move_right(DOC, &blocks, 5), 6);
        assert_eq!(move_right(DOC, &blocks, 6), 8, "end of block jumps to next");
        assert_eq!(move_right(DOC, &blocks, 24), 24, "stays at document end");
    }

    #[test]
    fn left_moves_within_block_then_jumps_gap() {
        let blocks = Blocks::parse(DOC);
        assert_eq!(move_left(DOC, &blocks, 9), 8);
        assert_eq!(
            move_left(DOC, &blocks, 8),
            6,
            "start of block jumps to previous"
        );
        assert_eq!(move_left(DOC, &blocks, 0), 0, "stays at document start");
    }

    #[test]
    fn movement_from_a_gap_lands_in_a_block() {
        let blocks = Blocks::parse(DOC);
        assert_eq!(move_right(DOC, &blocks, 7), 8);
        assert_eq!(move_left(DOC, &blocks, 7), 6);
    }

    #[test]
    fn word_movement_stays_in_block_then_jumps() {
        let blocks = Blocks::parse(DOC);
        assert_eq!(move_word_right(DOC, &blocks, 8), 12);
        assert_eq!(move_word_right(DOC, &blocks, 12), 16);
        assert_eq!(move_word_right(DOC, &blocks, 16), 18);
        assert_eq!(move_word_left(DOC, &blocks, 18), 16);
        assert_eq!(move_word_left(DOC, &blocks, 16), 13);
        assert_eq!(move_word_left(DOC, &blocks, 2), 0, "`#` isn't a word");
    }

    #[test]
    fn document_bounds() {
        let blocks = Blocks::parse("\n\n# Head\n\nlast\n\n");
        assert_eq!(document_start(&blocks), 2);
        assert_eq!(document_end(&blocks), 14);
        let empty = Blocks::parse("");
        assert_eq!(document_start(&empty), 0);
        assert_eq!(document_end(&empty), 0);
    }

    #[test]
    fn adjacent_blocks_without_blank_line() {
        // "# Head" is 0..6, "Para" is 7..11.
        let text = "# Head\nPara";
        let blocks = Blocks::parse(text);
        assert_eq!(move_right(text, &blocks, 6), 7);
        assert_eq!(move_left(text, &blocks, 7), 6);
    }
}
