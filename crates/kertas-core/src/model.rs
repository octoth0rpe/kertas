//! Everything the view needs to display a document, kept up to date
//! incrementally as the text changes.

use std::ops::Range;
use std::rc::Rc;

use crate::blocks::{BlockKind, Blocks, item_ranges, normalize_label};
use crate::definitions::Definitions;
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
    /// The document's link reference definitions.
    pub definitions: Rc<Definitions>,
    /// The footnotes, as drawn at the end of the document (#90).
    pub notes: Rc<Vec<Note>>,
    /// The footnote references, in order: each one's label (normalized)
    /// and where it starts.
    pub footnote_references: Rc<Vec<(String, usize)>>,
    /// Whether any image in the document is from the web (#97).
    pub web_images: bool,
}

/// A footnote, as drawn with the others at the end of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Its number, or `None` if the footnote is never referenced.
    pub number: Option<usize>,
    /// Its label, as written.
    pub label: String,
    /// The item its definition is in.
    pub item: usize,
    /// Where that item's block starts, which its text maps into.
    pub block_start: usize,
    pub children: Vec<RenderedBlock>,
    /// How many references there are to it, each with a back-link.
    pub references: usize,
}

/// Which list items changed in an update: `old` (indices into the old
/// items) were replaced by `new_count` items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    pub old: Range<usize>,
    pub new_count: usize,
    /// How the notes changed, if they did: how many there were, and how
    /// many there are now.
    pub notes: Option<(usize, usize)>,
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
        let definitions = blocks.definitions().clone();
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

        let items: Vec<Item> = (0..new_len)
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

        let notes = notes(&items, &definitions, blocks.footnote_references());
        let old_len = old.map_or(0, |old| old.items.len());
        let old_notes = old.map_or(0, |old| old.notes.len());
        let splice = Splice {
            old: prefix..old_len - suffix,
            new_count: new_len - prefix - suffix,
            notes: old
                .is_none_or(|old| !look_the_same(&old.notes, &notes))
                .then_some((old_notes, notes.len())),
        };
        let web_images = items.iter().any(|item| item.rendered.has_web_image());
        let model = Self {
            web_images,
            text: text.into(),
            items,
            definitions: Rc::new(definitions),
            notes: Rc::new(notes),
            footnote_references: Rc::new(blocks.footnote_references().to_vec()),
        };
        (model, splice)
    }
}

/// Whether two sets of notes are drawn the same, wherever their
/// definitions are.
fn look_the_same(old: &[Note], new: &[Note]) -> bool {
    old.len() == new.len()
        && old.iter().zip(new).all(|(old, new)| {
            old.number == new.number
                && old.label == new.label
                && old.references == new.references
                && old.children == new.children
        })
}

/// The footnotes defined in `items`: those referenced, by number, then
/// the rest in the order they're defined. If a label is defined twice, the
/// first definition is the one referenced.
fn notes(items: &[Item], definitions: &Definitions, references: &[(String, usize)]) -> Vec<Note> {
    let mut notes = Vec::new();
    for (index, item) in items.iter().enumerate() {
        collect_notes(&item.rendered, index, item.block.start, &mut notes);
    }
    let mut numbered = std::collections::HashSet::new();
    for note in &mut notes {
        let label = normalize_label(&note.label);
        if numbered.insert(label.clone()) {
            note.number = definitions.footnote_number(&label);
            note.references = references.iter().filter(|(l, _)| *l == label).count();
        }
    }
    // A stable sort, so unnumbered notes stay in source order.
    notes.sort_by_key(|note| note.number.unwrap_or(usize::MAX));
    notes
}

/// The footnote definitions in `block`, including those nested in lists
/// and quotes, unnumbered.
fn collect_notes(block: &RenderedBlock, item: usize, block_start: usize, out: &mut Vec<Note>) {
    match block {
        RenderedBlock::FootnoteDefinition { label, children } => out.push(Note {
            number: None,
            label: label.clone(),
            item,
            block_start,
            children: children.clone(),
            references: 0,
        }),
        RenderedBlock::List { items, .. } => {
            for child in items.iter().flat_map(|list_item| &list_item.children) {
                collect_notes(child, item, block_start, out);
            }
        }
        RenderedBlock::BlockQuote(children) | RenderedBlock::Alert { children, .. } => {
            for child in children {
                collect_notes(child, item, block_start, out);
            }
        }
        _ => {}
    }
}

fn render_item(
    text: &str,
    block: &Range<usize>,
    kind: BlockKind,
    definitions: &Definitions,
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
                new_count: 1,
                notes: None
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
                new_count: 2,
                notes: None
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
                new_count: 1,
                notes: None
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
                new_count: 1,
                notes: None
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
                new_count: 2,
                notes: None
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
                new_count: 1,
                notes: None
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
            "/../../spikes/fixtures/10k.md"
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
        // Typically 13 ms in a debug build and 1.2 ms in release (M2,
        // 2026-10-13); the bound leaves room for a slow or busy machine.
        assert!(elapsed.as_millis() < 100, "took {elapsed:?}");
        eprintln!("updated 10k-line model after one keystroke in {elapsed:?}");
    }

    #[test]
    fn notes_are_numbered_then_unreferenced() {
        let model =
            Model::new("[^draft]: Not used.\n\nB[^b], A[^a], B[^b].\n\n[^a]: Ay.\n\n[^b]: Bee.\n");
        let notes: Vec<(Option<usize>, &str, usize, usize)> = model
            .notes
            .iter()
            .map(|note| (note.number, note.label.as_str(), note.item, note.references))
            .collect();
        assert_eq!(
            notes,
            [
                (Some(1), "b", 3, 2),
                (Some(2), "a", 2, 1),
                (None, "draft", 0, 0)
            ]
        );
        assert_eq!(
            model.notes[0].block_start,
            model.text.find("[^b]:").unwrap()
        );
    }

    #[test]
    fn a_label_defined_twice_numbers_the_first() {
        let model = Model::new("X[^a]\n\n[^a]: One.\n\n[^A]: Two.\n");
        let numbers: Vec<Option<usize>> = model.notes.iter().map(|note| note.number).collect();
        assert_eq!(numbers, [Some(1), None]);
    }

    #[test]
    fn notes_change_only_when_they_look_different() {
        let before = "Intro.\n\nText[^a].\n\n[^a]: Note.\n";
        // Typing above them moves the notes' definitions, but they look
        // the same.
        assert_eq!(check_edit(before, &format!("More. {before}")).notes, None);
        assert_eq!(
            check_edit(before, "Intro.\n\nText[^a].\n\n[^a]: Notes.\n").notes,
            Some((1, 1))
        );
        assert_eq!(
            check_edit(before, "Intro.\n\nText[^a].\n\n[^a]: Note.\n\n[^b]: B.\n").notes,
            Some((1, 2))
        );
        // Numbers change, so everything is drawn again.
        let splice = check_edit(
            before,
            "Intro[^b].\n\nText[^a].\n\n[^a]: Note.\n\n[^b]: B.\n",
        );
        assert_eq!(splice.old, 0..3);
        assert_eq!(splice.notes, Some((1, 2)));
    }

    #[test]
    fn web_images_are_noticed_anywhere() {
        assert!(!Model::new("![a](a.png) and [link](https://example.com)\n").web_images);
        for text in [
            "![a](https://example.com/a.png)\n",
            "- item\n  > ![a](http://example.com/a.png)\n",
            "| a |\n|---|\n| ![a](https://example.com/a.png) |\n",
            "Text[^a].\n\n[^a]: ![a](https://example.com/a.png)\n",
            "![a][r]\n\n[r]: https://example.com/a.png\n",
        ] {
            assert!(Model::new(text).web_images, "{text}");
        }
        let (model, _) = Model::new("Text.\n").updated("Text ![a](https://x.org/a.png).\n");
        assert!(model.web_images);
    }
}
