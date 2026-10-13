//! Everything the view needs to display a document, kept up to date
//! incrementally as the text changes.

use std::ops::Range;
use std::rc::Rc;

use crate::blocks::{BlockKind, Blocks, item_ranges};
use crate::rendered::{RenderedBlock, render_block};

/// One list item: a block plus the blank lines after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The source range shown when the item is revealed. Items cover the
    /// whole document; see [`item_ranges`].
    pub range: Range<usize>,
    /// The block's own range, which rendered text maps into.
    pub block: Range<usize>,
    pub kind: BlockKind,
    pub rendered: Rc<RenderedBlock>,
}

/// Where the caret is relative to an item's block when it's on a blank
/// line that isn't part of the block's source (see [`Model::source_range`]).
/// Typing there starts a new block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    Above,
    Below,
}

/// A parsed, rendered document.
#[derive(Debug, Clone)]
pub struct Model {
    pub text: Rc<str>,
    pub items: Vec<Item>,
    /// The source of the document's link reference definitions.
    pub definitions: Rc<str>,
}

/// Which list items changed in an update: `old` (indices into the old
/// items) were replaced by `new_count` items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    pub old: Range<usize>,
    pub new_count: usize,
}

impl Model {
    pub fn new(text: &str) -> Self {
        Self::build(text, None).0
    }

    /// The model for `text` after an edit, reusing unchanged items from
    /// `self`, and which items changed.
    pub fn updated(&self, text: &str) -> (Self, Splice) {
        Self::build(text, Some(self))
    }

    /// The index of the item containing `offset`.
    pub fn item_at(&self, offset: usize) -> usize {
        self.items
            .partition_point(|item| item.range.start <= offset)
            .saturating_sub(1)
    }

    /// The source shown when item `index` is revealed: its block, plus the
    /// line right after it. Typing on that line still continues the block.
    /// The blank lines beyond it, and any before the first block, are gaps.
    pub fn source_range(&self, index: usize) -> Range<usize> {
        let item = &self.items[index];
        if item.block.is_empty() {
            // A document with no blocks: all of it is source.
            return item.range.clone();
        }
        let next_line = self.text[item.block.end..item.range.end]
            .find('\n')
            .map(|newline| item.block.end + newline + 1);
        let end = match next_line {
            Some(start) => {
                let end = self.text[start..item.range.end]
                    .find('\n')
                    .map_or(item.range.end, |newline| start + newline);
                if end > start && self.text.as_bytes()[end - 1] == b'\r' {
                    end - 1
                } else {
                    end
                }
            }
            None => item.range.end,
        };
        // The block's first line, which may start before the block (after
        // a byte-order mark, say).
        let start = self.text[..item.block.start]
            .rfind('\n')
            .map_or(0, |newline| newline + 1)
            .max(item.range.start);
        start..end
    }

    /// Whether `offset`, in item `index`, is in a gap before or after the
    /// item's source.
    pub fn gap_at(&self, index: usize, offset: usize) -> Option<Gap> {
        let source = self.source_range(index);
        if offset < source.start {
            Some(Gap::Above)
        } else if offset > source.end {
            Some(Gap::Below)
        } else {
            None
        }
    }

    fn build(text: &str, old: Option<&Model>) -> (Self, Splice) {
        let blocks = Blocks::parse(text);
        let definitions = definitions(text, &blocks);
        let ranges = item_ranges(text, &blocks);

        let block_info: Vec<(Range<usize>, BlockKind)> = if blocks.is_empty() {
            vec![(0..0, BlockKind::Paragraph)]
        } else {
            blocks
                .as_slice()
                .iter()
                .map(|block| (block.range.clone(), block.kind))
                .collect()
        };

        // Items whose source, kind and offset are unchanged keep their
        // rendering. Link definitions affect every block, so if they changed,
        // everything is re-rendered.
        let reusable = old.filter(|old| *old.definitions == definitions);
        let new_len = ranges.len();
        let same = |old: &Model, old_index: usize, new_index: usize, shift: isize| {
            let old_item = &old.items[old_index];
            let new_range = &ranges[new_index];
            old_item.kind == block_info[new_index].1
                && old_item.range.start as isize + shift == new_range.start as isize
                && old_item.block.start as isize + shift == block_info[new_index].0.start as isize
                && old.text[old_item.range.clone()] == text[new_range.clone()]
        };
        let (prefix, suffix) = match reusable {
            Some(old) => {
                let shift = text.len() as isize - old.text.len() as isize;
                let max = old.items.len().min(new_len);
                let prefix = (0..max).take_while(|&i| same(old, i, i, 0)).count();
                let suffix = (0..max - prefix)
                    .take_while(|&i| same(old, old.items.len() - 1 - i, new_len - 1 - i, shift))
                    .count();
                (prefix, suffix)
            }
            None => (0, 0),
        };

        let items = (0..new_len)
            .map(|index| {
                let (block, kind) = block_info[index].clone();
                let reused = reusable.and_then(|old| {
                    if index < prefix {
                        Some(old.items[index].rendered.clone())
                    } else if index >= new_len - suffix {
                        Some(
                            old.items[old.items.len() - (new_len - index)]
                                .rendered
                                .clone(),
                        )
                    } else {
                        None
                    }
                });
                let rendered = reused
                    .unwrap_or_else(|| Rc::new(render_item(text, &block, kind, &definitions)));
                Item {
                    range: ranges[index].clone(),
                    block,
                    kind,
                    rendered,
                }
            })
            .collect();

        let old_len = old.map_or(0, |old| old.items.len());
        let splice = Splice {
            old: prefix..old_len - suffix,
            new_count: new_len - prefix - suffix,
        };
        let model = Self {
            text: text.into(),
            items,
            definitions: definitions.into(),
        };
        (model, splice)
    }
}

fn render_item(
    text: &str,
    block: &Range<usize>,
    kind: BlockKind,
    definitions: &str,
) -> RenderedBlock {
    let source = &text[block.clone()];
    match kind {
        BlockKind::FrontMatter | BlockKind::LinkReferenceDefinition => {
            RenderedBlock::Raw(source.to_owned())
        }
        _ if source.is_empty() => RenderedBlock::Paragraph(Default::default()),
        _ => render_block(source, definitions)
            .unwrap_or_else(|| RenderedBlock::Raw(source.to_owned())),
    }
}

fn definitions(text: &str, blocks: &Blocks) -> String {
    blocks
        .as_slice()
        .iter()
        .filter(|block| block.kind == BlockKind::LinkReferenceDefinition)
        .map(|block| &text[block.range.clone()])
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies an edit and checks the incremental result matches a fresh
    /// build, returning the splice.
    #[track_caller]
    fn check_edit(before: &str, after: &str) -> Splice {
        let old = Model::new(before);
        let (updated, splice) = old.updated(after);
        let fresh = Model::new(after);
        assert_eq!(updated.items, fresh.items, "incremental update differs");
        assert_eq!(
            old.items.len() - splice.old.len() + splice.new_count,
            fresh.items.len()
        );
        splice
    }

    #[test]
    fn typing_in_a_paragraph_changes_one_item() {
        let splice = check_edit("# H\n\nPara\n\n- a\n", "# H\n\nParas\n\n- a\n");
        assert_eq!(
            splice,
            Splice {
                old: 1..2,
                new_count: 1
            }
        );
    }

    #[test]
    fn splitting_a_paragraph_adds_an_item() {
        let splice = check_edit("One two\n\nEnd", "One\n\ntwo\n\nEnd");
        assert_eq!(
            splice,
            Splice {
                old: 0..1,
                new_count: 2
            }
        );
    }

    #[test]
    fn merging_paragraphs_removes_an_item() {
        let splice = check_edit("A\n\nB\n\nC", "A\nB\n\nC");
        assert_eq!(
            splice,
            Splice {
                old: 0..2,
                new_count: 1
            }
        );
    }

    #[test]
    fn typing_a_blank_line_changes_only_that_item() {
        let splice = check_edit("A\n\nB", "A\n\n\nB");
        assert_eq!(
            splice,
            Splice {
                old: 0..1,
                new_count: 1
            }
        );
    }

    #[test]
    fn changing_a_definition_rerenders_everything() {
        let splice = check_edit("[a][k]\n\n[k]: /x\n", "[a][k]\n\n[k]: /y\n");
        assert_eq!(
            splice,
            Splice {
                old: 0..2,
                new_count: 2
            }
        );
    }

    #[test]
    fn heading_level_change() {
        check_edit("# A\n\nB", "## A\n\nB");
    }

    #[test]
    fn emptying_and_filling_the_document() {
        assert_eq!(
            check_edit("A", ""),
            Splice {
                old: 0..1,
                new_count: 1
            }
        );
        check_edit("", "A");
        check_edit("\n\n", "\n\nA");
    }

    #[test]
    fn item_at_maps_offsets() {
        let model = Model::new("A\n\nB");
        assert_eq!(model.item_at(2), 0);
        assert_eq!(model.item_at(3), 1);
    }

    #[test]
    fn gaps_after_a_blank_line() {
        let model = Model::new("Para\n");
        assert_eq!(model.source_range(0), 0..5);
        assert_eq!(
            model.gap_at(0, 5),
            None,
            "typing here continues the paragraph"
        );

        let model = Model::new("Para\n\n");
        assert_eq!(model.source_range(0), 0..5);
        assert_eq!(model.gap_at(0, 6), Some(Gap::Below));

        let model = Model::new("A\n\nB");
        assert_eq!(model.source_range(0), 0..2);
        assert_eq!(model.gap_at(0, 2), None, "the separating blank line");

        let model = Model::new("A\n\n\n\nB");
        assert_eq!(model.source_range(0), 0..2);
        assert_eq!(model.gap_at(0, 3), Some(Gap::Below));
        assert_eq!(model.gap_at(0, 4), Some(Gap::Below));

        let model = Model::new("A\r\n\r\n");
        assert_eq!(model.source_range(0), 0..3);
        assert_eq!(model.gap_at(0, 5), Some(Gap::Below));
    }

    #[test]
    fn gap_above_the_first_block() {
        let model = Model::new("\n\nA");
        assert_eq!(model.source_range(0), 2..3);
        assert_eq!(model.gap_at(0, 0), Some(Gap::Above));
        assert_eq!(model.gap_at(0, 2), None);
    }

    #[test]
    fn byte_order_mark_is_not_a_gap() {
        let model = Model::new("\u{FEFF}A");
        assert_eq!(model.gap_at(0, 0), None);
    }

    #[test]
    fn no_gaps_without_blocks() {
        let model = Model::new("\n\n\n");
        assert_eq!(model.source_range(0), 0..3);
        assert_eq!(model.gap_at(0, 3), None);
    }

    #[test]
    fn typing_in_large_document_is_fast() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/spikes/fixtures/10k.md"
        ))
        .unwrap();
        let model = Model::new(&text);
        let middle = text.len() / 2;
        let middle = (middle..).find(|&i| text.is_char_boundary(i)).unwrap();
        let edited = format!("{}x{}", &text[..middle], &text[middle..]);
        let start = std::time::Instant::now();
        let (_, splice) = model.updated(&edited);
        let elapsed = start.elapsed();
        assert!(splice.new_count <= 2, "{splice:?}");
        eprintln!("updated 10k-line model after one keystroke in {elapsed:?}");
    }
}
