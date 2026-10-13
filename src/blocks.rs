//! Splits a Markdown document into top-level blocks.
//!
//! Each block knows its kind and its byte range in the source. This is what
//! per-block reveal (C4) works from: the block containing the caret is shown
//! as raw source, and every other block is rendered.
//!
//! Ranges are normalized so they cover exactly the block's own lines:
//! - They start at the beginning of the block's first line, including any
//!   leading indentation.
//! - They end after the block's last character, excluding trailing line
//!   endings and blank lines.
//!
//! Blank lines between blocks belong to no block.
//!
//! Front matter is detected here rather than by pulldown-cmark, which
//! accepts metadata blocks anywhere in a document. Kertas only treats it as
//! front matter at the very start of the file, after an optional byte-order
//! mark. Elsewhere, `---` lines are thematic breaks or setext underlines.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

use crate::definitions::Definitions;

/// The kind of a top-level block. See REQUIREMENTS §1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// ATX (`# Heading`) or setext (underlined) heading, level 1–6.
    Heading {
        level: u8,
    },
    /// A whole list, including nested items and lists.
    List {
        ordered: bool,
    },
    /// A whole blockquote, including nested content.
    BlockQuote,
    CodeBlock {
        fenced: bool,
    },
    Table,
    ThematicBreak,
    HtmlBlock,
    /// YAML (`---`) or TOML (`+++`) front matter at the start of the file.
    FrontMatter,
    /// A link reference definition, e.g. `[label]: https://example.com`.
    LinkReferenceDefinition,
}

/// A top-level block and its byte range in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub range: Range<usize>,
}

/// The parser options Kertas uses everywhere: CommonMark plus the GFM
/// extensions in M2. Front matter is handled separately; see the module docs.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

const BYTE_ORDER_MARK: char = '\u{FEFF}';

/// The top-level blocks of a document, in source order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blocks {
    blocks: Vec<Block>,
    /// The document's link reference definitions, wherever they are.
    definitions: Definitions,
}

impl Blocks {
    /// Parses `text` into its top-level blocks.
    pub fn parse(text: &str) -> Self {
        let mut blocks = Vec::new();

        // The Markdown body starts after the byte-order mark and front
        // matter, if present. pulldown-cmark parses only the body, so its
        // ranges are shifted by `body_start`.
        let content_start = if text.starts_with(BYTE_ORDER_MARK) {
            BYTE_ORDER_MARK.len_utf8()
        } else {
            0
        };
        let mut body_start = content_start;
        if let Some(len) = front_matter_len(&text[content_start..]) {
            body_start += len;
            blocks.push(Block {
                kind: BlockKind::FrontMatter,
                range: content_start..body_start,
            });
        }
        let body = &text[body_start..];
        let shift = |range: Range<usize>| range.start + body_start..range.end + body_start;
        let normalize = |range: Range<usize>| normalize(text, content_start, shift(range));

        let mut depth = 0usize;
        let mut iter = Parser::new_ext(body, parser_options()).into_offset_iter();

        for (event, range) in iter.by_ref() {
            match event {
                Event::Start(tag) => {
                    if depth == 0
                        && let Some(kind) = block_kind(&tag)
                    {
                        blocks.push(Block {
                            kind,
                            range: normalize(range),
                        });
                    }
                    depth += 1;
                }
                Event::End(_) => depth -= 1,
                Event::Rule if depth == 0 => blocks.push(Block {
                    kind: BlockKind::ThematicBreak,
                    range: normalize(range),
                }),
                _ => {}
            }
        }

        let link_definitions = Definitions::from_parser(iter.reference_definitions());
        // Link reference definitions produce no events, so add them from the
        // parser's definition table. That table also includes definitions
        // nested inside lists and blockquotes; those are already part of
        // their enclosing block, so skip them.
        let mut definitions: Vec<Block> = iter
            .reference_definitions()
            .iter()
            .map(|(_, definition)| Block {
                kind: BlockKind::LinkReferenceDefinition,
                range: normalize(definition.span.clone()),
            })
            .filter(|definition| {
                !blocks.iter().any(|block| {
                    block.range.start <= definition.range.start
                        && definition.range.end <= block.range.end
                })
            })
            .collect();
        blocks.append(&mut definitions);
        blocks.sort_by_key(|block| block.range.start);

        Self {
            blocks,
            definitions: link_definitions,
        }
    }

    /// The document's link reference definitions, for rendering blocks on
    /// their own.
    pub fn definitions(&self) -> &Definitions {
        &self.definitions
    }

    /// All blocks, in source order.
    pub fn as_slice(&self) -> &[Block] {
        &self.blocks
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The index of the block containing byte `offset`, if any.
    ///
    /// A block's end offset counts as inside it, so a caret placed just
    /// after a block's last character is in that block. Offsets in the blank
    /// lines between blocks are in no block.
    pub fn index_at(&self, offset: usize) -> Option<usize> {
        let index = self
            .blocks
            .partition_point(|block| block.range.start <= offset)
            .checked_sub(1)?;
        (offset <= self.blocks[index].range.end).then_some(index)
    }

    /// The block containing byte `offset`, if any. See [`Self::index_at`].
    pub fn at(&self, offset: usize) -> Option<&Block> {
        self.index_at(offset).map(|index| &self.blocks[index])
    }
}

/// The source range each list item covers when shown, one per block.
///
/// Unlike block ranges, these cover the whole document, so every offset
/// belongs to an item and can hold the caret:
/// - The first item starts at offset 0, including leading blank lines.
/// - Each item extends over the blank lines after its block, up to (but not
///   including) the line ending just before the next block.
/// - The last item extends to the end of the document.
///
/// A document with no blocks (empty, or only blank lines) has one item
/// covering all of it.
///
/// An offset belongs to the last item starting at or before it; see
/// [`item_at`].
pub fn item_ranges(text: &str, blocks: &Blocks) -> Vec<Range<usize>> {
    let blocks = blocks.as_slice();
    if blocks.is_empty() {
        return vec![0..text.len()];
    }
    let bytes = text.as_bytes();
    (0..blocks.len())
        .map(|index| {
            let start = if index == 0 {
                0
            } else {
                blocks[index].range.start
            };
            let end = match blocks.get(index + 1) {
                Some(next) => {
                    let mut end = next.range.start;
                    if end > 0 && bytes[end - 1] == b'\n' {
                        end -= 1;
                    }
                    if end > 0 && bytes[end - 1] == b'\r' {
                        end -= 1;
                    }
                    end.max(blocks[index].range.end)
                }
                None => text.len(),
            };
            start..end
        })
        .collect()
}

/// The index of the item containing `offset`: the last one starting at or
/// before it.
pub fn item_at(items: &[Range<usize>], offset: usize) -> usize {
    items
        .partition_point(|item| item.start <= offset)
        .saturating_sub(1)
}

fn block_kind(tag: &Tag) -> Option<BlockKind> {
    Some(match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { level, .. } => BlockKind::Heading {
            level: heading_level(*level),
        },
        Tag::List(start) => BlockKind::List {
            ordered: start.is_some(),
        },
        Tag::BlockQuote(_) => BlockKind::BlockQuote,
        Tag::CodeBlock(kind) => BlockKind::CodeBlock {
            fenced: matches!(kind, CodeBlockKind::Fenced(_)),
        },
        Tag::Table(_) => BlockKind::Table,
        Tag::HtmlBlock => BlockKind::HtmlBlock,
        // Other tags are inline, nested-only, or belong to extensions Kertas
        // doesn't enable.
        _ => return None,
    })
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The length of the front matter at the start of `text`, if any, up to
/// the end of its closing delimiter line (excluding the line ending).
///
/// Front matter opens with a `---` (YAML) or `+++` (TOML) line, and closes
/// with the same delimiter (or `...` for YAML). The line after the opening
/// delimiter must not be blank, so a thematic break at the top of a
/// document isn't mistaken for front matter.
fn front_matter_len(text: &str) -> Option<usize> {
    let mut lines = text.split_inclusive('\n');
    let opening = lines.next()?;
    let delimiter = opening.trim_end();
    if delimiter != "---" && delimiter != "+++" {
        return None;
    }

    let mut offset = opening.len();
    for (index, line) in lines.enumerate() {
        let trimmed = line.trim_end();
        if index == 0 && trimmed.is_empty() {
            return None;
        }
        if trimmed == delimiter || (delimiter == "---" && trimmed == "...") {
            return Some(offset + trimmed.len());
        }
        offset += line.len();
    }
    None
}

/// Extends `range` back to the start of its line over leading indentation,
/// and trims trailing line endings. `content_start` is where the first line
/// starts, after any byte-order mark.
fn normalize(text: &str, content_start: usize, range: Range<usize>) -> Range<usize> {
    let bytes = text.as_bytes();

    let mut start = range.start;
    let indent = bytes[content_start..start]
        .iter()
        .rev()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    let line_start = start - indent;
    if line_start == content_start || matches!(bytes[line_start - 1], b'\n' | b'\r') {
        start = line_start;
    }

    let mut end = range.end;
    while end > start && matches!(bytes[end - 1], b'\n' | b'\r') {
        end -= 1;
    }

    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use BlockKind::*;

    /// Parses `text` and returns each block's kind and source text.
    fn parse(text: &str) -> Vec<(BlockKind, &str)> {
        Blocks::parse(text)
            .as_slice()
            .iter()
            .map(|block| (block.kind, &text[block.range.clone()]))
            .collect()
    }

    #[test]
    fn empty_document() {
        assert!(Blocks::parse("").is_empty());
        assert!(Blocks::parse("\n\n  \n").is_empty());
    }

    #[test]
    fn paragraphs() {
        assert_eq!(
            parse("One line.\n\nTwo\nlines.\n"),
            [(Paragraph, "One line."), (Paragraph, "Two\nlines.")]
        );
    }

    #[test]
    fn atx_headings() {
        assert_eq!(
            parse("# One\n## Two\n###### Six"),
            [
                (Heading { level: 1 }, "# One"),
                (Heading { level: 2 }, "## Two"),
                (Heading { level: 6 }, "###### Six"),
            ]
        );
    }

    #[test]
    fn setext_headings() {
        assert_eq!(
            parse("Title\n=====\n\nSub\n---\n"),
            [
                (Heading { level: 1 }, "Title\n====="),
                (Heading { level: 2 }, "Sub\n---"),
            ]
        );
    }

    #[test]
    fn whole_list_is_one_block() {
        let text = "- a\n  - nested\n\n- loose\n\n1. first\n2. second\n";
        assert_eq!(
            parse(text),
            [
                (List { ordered: false }, "- a\n  - nested\n\n- loose"),
                (List { ordered: true }, "1. first\n2. second"),
            ]
        );
    }

    #[test]
    fn task_list() {
        assert_eq!(
            parse("- [ ] todo\n- [x] done\n"),
            [(List { ordered: false }, "- [ ] todo\n- [x] done")]
        );
    }

    #[test]
    fn indented_list_includes_indent() {
        assert_eq!(
            parse("  - indented\n  - list\n"),
            [(List { ordered: false }, "  - indented\n  - list")]
        );
    }

    #[test]
    fn whole_blockquote_is_one_block() {
        assert_eq!(
            parse("> quote\n> > nested\n>\n> - list in quote\n"),
            [(BlockQuote, "> quote\n> > nested\n>\n> - list in quote")]
        );
    }

    #[test]
    fn fenced_code_block() {
        assert_eq!(
            parse("```rust\nfn main() {}\n\n```\n~~~\ntilde\n~~~"),
            [
                (CodeBlock { fenced: true }, "```rust\nfn main() {}\n\n```"),
                (CodeBlock { fenced: true }, "~~~\ntilde\n~~~"),
            ]
        );
    }

    #[test]
    fn unclosed_fence_runs_to_end() {
        assert_eq!(
            parse("```\nnever closed\n\nstill code\n"),
            [(
                CodeBlock { fenced: true },
                "```\nnever closed\n\nstill code"
            )]
        );
    }

    #[test]
    fn indented_code_block_includes_indent() {
        assert_eq!(
            parse("Para\n\n    code\n    more\n"),
            [
                (Paragraph, "Para"),
                (CodeBlock { fenced: false }, "    code\n    more"),
            ]
        );
    }

    #[test]
    fn table() {
        assert_eq!(
            parse("| a | b |\n|---|--:|\n| 1 | 2 |\n"),
            [(Table, "| a | b |\n|---|--:|\n| 1 | 2 |")]
        );
    }

    #[test]
    fn thematic_breaks() {
        assert_eq!(
            parse("***\n\n- - -\n\n___"),
            [
                (ThematicBreak, "***"),
                (ThematicBreak, "- - -"),
                (ThematicBreak, "___"),
            ]
        );
    }

    #[test]
    fn html_block() {
        assert_eq!(
            parse("<div>\nhi\n</div>\n\nafter"),
            [(HtmlBlock, "<div>\nhi\n</div>"), (Paragraph, "after")]
        );
    }

    #[test]
    fn yaml_front_matter() {
        assert_eq!(
            parse("---\ntitle: kertas\n---\n\n# Heading\n"),
            [
                (FrontMatter, "---\ntitle: kertas\n---"),
                (Heading { level: 1 }, "# Heading"),
            ]
        );
    }

    #[test]
    fn toml_front_matter() {
        assert_eq!(
            parse("+++\ntitle = \"kertas\"\n+++\n\nText"),
            [
                (FrontMatter, "+++\ntitle = \"kertas\"\n+++"),
                (Paragraph, "Text")
            ]
        );
    }

    #[test]
    fn yaml_front_matter_closed_with_dots() {
        assert_eq!(
            parse("---\ntitle: kertas\n...\nText"),
            [
                (FrontMatter, "---\ntitle: kertas\n..."),
                (Paragraph, "Text")
            ]
        );
    }

    #[test]
    fn empty_front_matter() {
        assert_eq!(
            parse("---\n---\nText"),
            [(FrontMatter, "---\n---"), (Paragraph, "Text")]
        );
    }

    #[test]
    fn unclosed_front_matter_is_not_front_matter() {
        assert_eq!(
            parse("---\ntitle: x\n"),
            [(ThematicBreak, "---"), (Paragraph, "title: x")]
        );
    }

    #[test]
    fn leading_thematic_break_is_not_front_matter() {
        assert_eq!(
            parse("---\n\nText\n\n---\n"),
            [
                (ThematicBreak, "---"),
                (Paragraph, "Text"),
                (ThematicBreak, "---"),
            ]
        );
    }

    #[test]
    fn front_matter_after_byte_order_mark() {
        let text = "\u{FEFF}---\nk: v\n---\n# H";
        assert_eq!(
            parse(text),
            [
                (FrontMatter, "---\nk: v\n---"),
                (Heading { level: 1 }, "# H")
            ]
        );
    }

    #[test]
    fn crlf_front_matter() {
        assert_eq!(
            parse("---\r\nk: v\r\n---\r\n\r\nText"),
            [(FrontMatter, "---\r\nk: v\r\n---"), (Paragraph, "Text")]
        );
    }

    #[test]
    fn link_reference_definitions() {
        assert_eq!(
            parse("See [kertas][k].\n\n[k]: https://example.com \"Title\"\n[other]: /path\n"),
            [
                (Paragraph, "See [kertas][k]."),
                (
                    LinkReferenceDefinition,
                    "[k]: https://example.com \"Title\""
                ),
                (LinkReferenceDefinition, "[other]: /path"),
            ]
        );
    }

    #[test]
    fn definition_inside_blockquote_is_not_separate() {
        assert_eq!(
            parse("> [k]: /path\n> text [k]\n"),
            [(BlockQuote, "> [k]: /path\n> text [k]")]
        );
    }

    #[test]
    fn adjacent_blocks_without_blank_lines() {
        assert_eq!(
            parse("# Heading\nParagraph\n```\ncode\n```\n> quote"),
            [
                (Heading { level: 1 }, "# Heading"),
                (Paragraph, "Paragraph"),
                (CodeBlock { fenced: true }, "```\ncode\n```"),
                (BlockQuote, "> quote"),
            ]
        );
    }

    #[test]
    fn crlf_line_endings_are_trimmed() {
        assert_eq!(
            parse("# Head\r\n\r\nPara\r\nline\r\n\r\n- a\r\n- b\r\n"),
            [
                (Heading { level: 1 }, "# Head"),
                (Paragraph, "Para\r\nline"),
                (List { ordered: false }, "- a\r\n- b"),
            ]
        );
    }

    #[test]
    fn non_ascii_ranges_are_on_char_boundaries() {
        // Slicing in `parse` would panic on a bad boundary.
        assert_eq!(
            parse("# 見出し\n\n段落 — café 🦀\n"),
            [
                (Heading { level: 1 }, "# 見出し"),
                (Paragraph, "段落 — café 🦀")
            ]
        );
    }

    #[test]
    fn indented_block_after_byte_order_mark_includes_indent() {
        let text = "\u{FEFF}  - item\n";
        assert_eq!(parse(text), [(List { ordered: false }, "  - item")]);
    }

    #[test]
    fn byte_order_mark_is_not_part_of_first_block() {
        let text = "\u{FEFF}# Heading\n";
        let blocks = Blocks::parse(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks.as_slice()[0].kind, Heading { level: 1 });
        assert_eq!(&text[blocks.as_slice()[0].range.clone()], "# Heading");
    }

    #[test]
    fn dashes_after_start_are_not_front_matter() {
        assert_eq!(
            parse(
                "Intro

---
title: x
---
"
            ),
            [
                (Paragraph, "Intro"),
                (ThematicBreak, "---"),
                (Heading { level: 2 }, "title: x\n---"),
            ]
        );
    }

    #[test]
    fn index_at_finds_containing_block() {
        let text = "# Head\n\nPara\n";
        let blocks = Blocks::parse(text);
        // "# Head" is 0..6, "Para" is 8..12.
        assert_eq!(blocks.index_at(0), Some(0));
        assert_eq!(blocks.index_at(3), Some(0));
        assert_eq!(blocks.index_at(6), Some(0), "end of block is inside it");
        assert_eq!(blocks.index_at(7), None, "blank line between blocks");
        assert_eq!(blocks.index_at(8), Some(1));
        assert_eq!(blocks.index_at(12), Some(1));
        assert_eq!(blocks.index_at(13), None, "trailing newline");
        assert_eq!(blocks.index_at(100), None);
    }

    #[test]
    fn index_at_adjacent_blocks() {
        let text = "# Head\nPara";
        let blocks = Blocks::parse(text);
        // "# Head" is 0..6, "Para" is 7..11. The newline belongs to neither.
        assert_eq!(blocks.index_at(6), Some(0));
        assert_eq!(blocks.index_at(7), Some(1));
        assert_eq!(blocks.at(9).map(|block| block.kind), Some(Paragraph));
    }

    fn items(text: &str) -> Vec<&str> {
        item_ranges(text, &Blocks::parse(text))
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn items_include_following_blank_lines() {
        assert_eq!(items("A\n\nB\n"), ["A\n", "B\n"]);
        assert_eq!(items("A\n\n\n\nB"), ["A\n\n\n", "B"]);
        assert_eq!(items("# H\nPara"), ["# H", "Para"]);
    }

    #[test]
    fn first_item_includes_leading_blank_lines() {
        assert_eq!(items("\n\nA\n\nB"), ["\n\nA\n", "B"]);
    }

    #[test]
    fn items_with_crlf() {
        assert_eq!(items("A\r\n\r\nB\r\n"), ["A\r\n", "B\r\n"]);
    }

    #[test]
    fn document_without_blocks_is_one_item() {
        assert_eq!(items(""), [""]);
        assert_eq!(items("\n\n"), ["\n\n"]);
    }

    #[test]
    fn items_cover_every_offset() {
        let text = "\n# H\n\n- a\n- b\n\n\n```\nx\n```\ntext\n";
        let ranges = item_ranges(text, &Blocks::parse(text));
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges.last().unwrap().end, text.len());
        for pair in ranges.windows(2) {
            // Only the single line ending before the next block is uncovered,
            // and the previous item's inclusive end reaches it.
            assert_eq!(pair[0].end + 1, pair[1].start, "{ranges:?}");
        }
    }

    #[test]
    fn item_at_finds_item() {
        let text = "A\n\nB";
        let ranges = item_ranges(text, &Blocks::parse(text));
        // "A\n" is 0..2, "B" is 3..4.
        assert_eq!(item_at(&ranges, 0), 0);
        assert_eq!(
            item_at(&ranges, 2),
            0,
            "blank line belongs to the item above"
        );
        assert_eq!(item_at(&ranges, 3), 1);
        assert_eq!(item_at(&ranges, 4), 1);
    }

    #[test]
    fn index_at_in_empty_document() {
        assert_eq!(Blocks::parse("").index_at(0), None);
    }
}
