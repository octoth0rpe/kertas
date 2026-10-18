//! What each top-level block looks like when rendered.
//!
//! Each top-level block is parsed on its own into a small tree: lists and
//! blockquotes contain nested blocks, and tables contain cells. Inline text
//! keeps source ranges relative to the start of its top-level block.

use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

#[cfg(test)]
use crate::blocks::{BlockKind, Blocks};
use crate::definitions::Definitions;
use crate::inline::{InlineBuilder, InlineText};
use crate::links::is_web;

/// Each heading level's text size, relative to body text, on screen and
/// in print.
pub const HEADING_SCALE: [f64; 6] = [2.0, 1.5, 1.25, 1.0, 0.875, 0.85];

/// The text size for a heading `level` (1 to 6), relative to body text.
pub fn heading_scale(level: u8) -> f64 {
    HEADING_SCALE[usize::from(level.clamp(1, 6)) - 1]
}

/// The font code is shown in, on screen and in print.
pub const CODE_FONT: &str = "Menlo";

/// A block prepared for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedBlock {
    Paragraph(InlineText),
    Heading {
        level: u8,
        text: InlineText,
    },
    ThematicBreak,
    List {
        /// The first item's number for an ordered list, `None` for bullets.
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    BlockQuote(Vec<RenderedBlock>),
    /// A GitHub alert: a blockquote whose first line is `[!NOTE]` or one of
    /// the other markers. The marker line isn't part of `children`.
    Alert {
        kind: AlertKind,
        children: Vec<RenderedBlock>,
    },
    CodeBlock {
        /// The info string's language, e.g. `rust` in ```` ```rust ````.
        language: Option<String>,
        /// The code, unstyled, with its mapping back to the source.
        code: InlineText,
    },
    Table(Table),
    /// An HTML block, shown as its source. It's never executed (N4).
    Html(String),
    /// A block that isn't rendered, shown as its source text: front matter
    /// and link reference definitions.
    Raw(String),
    /// A footnote's text (`[^label]: …`). It's drawn with the other notes
    /// at the end of the document, not where it is (#90).
    FootnoteDefinition {
        /// The label, as written.
        label: String,
        children: Vec<RenderedBlock>,
    },
}

impl RenderedBlock {
    /// Whether any image in the block is from the web (#97).
    pub fn has_web_image(&self) -> bool {
        let mut found = false;
        self.each_text(&mut |text| {
            found = found || text.images.iter().any(|image| is_web(&image.destination));
        });
        found
    }

    /// Calls `f` with each run of inline text in the block, nested blocks
    /// included.
    pub fn each_text(&self, f: &mut dyn FnMut(&InlineText)) {
        match self {
            Self::Paragraph(text) | Self::Heading { text, .. } => f(text),
            Self::CodeBlock { code, .. } => f(code),
            Self::List { items, .. } => {
                for child in items.iter().flat_map(|item| &item.children) {
                    child.each_text(f);
                }
            }
            Self::BlockQuote(children)
            | Self::Alert { children, .. }
            | Self::FootnoteDefinition { children, .. } => {
                for child in children {
                    child.each_text(f);
                }
            }
            Self::Table(table) => {
                for cell in table.header.iter().chain(table.rows.iter().flatten()) {
                    f(cell);
                }
            }
            Self::ThematicBreak | Self::Html(_) | Self::Raw(_) => {}
        }
    }
}

/// Which GitHub alert a blockquote is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    /// The title shown above the alert, as GitHub shows it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

impl From<BlockQuoteKind> for AlertKind {
    fn from(kind: BlockQuoteKind) -> Self {
        match kind {
            BlockQuoteKind::Note => Self::Note,
            BlockQuoteKind::Tip => Self::Tip,
            BlockQuoteKind::Important => Self::Important,
            BlockQuoteKind::Warning => Self::Warning,
            BlockQuoteKind::Caution => Self::Caution,
        }
    }
}

/// One item in a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The checkbox, for a task list item.
    pub task: Option<Task>,
    pub children: Vec<RenderedBlock>,
}

/// A task list item's checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    pub checked: bool,
    /// Where its `[ ]` or `[x]` starts in the block's source.
    pub marker: usize,
}

/// A GFM table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub alignments: Vec<ColumnAlignment>,
    pub header: Vec<InlineText>,
    pub rows: Vec<Vec<InlineText>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnAlignment {
    None,
    Left,
    Center,
    Right,
}

/// Prepares every block in `text` for display. A test helper: the app
/// renders each block as needed (see [`crate::model::Model`]).
#[cfg(test)]
pub(crate) fn render_blocks(text: &str, blocks: &Blocks) -> Vec<RenderedBlock> {
    let definitions = blocks.definitions();
    blocks
        .as_slice()
        .iter()
        .map(|block| {
            let source = &text[block.range.clone()];
            match block.kind {
                BlockKind::FrontMatter | BlockKind::LinkReferenceDefinition => {
                    RenderedBlock::Raw(source.to_owned())
                }
                _ => render_block(source, definitions)
                    .unwrap_or_else(|| RenderedBlock::Raw(source.to_owned())),
            }
        })
        .collect()
}

/// Renders a single top-level block from its source. `definitions` are the
/// document's link reference definitions, so reference links resolve.
pub fn render_block(source: &str, definitions: &Definitions) -> Option<RenderedBlock> {
    let mut builder = TreeBuilder::new(definitions);
    let text = definitions.prepare(source);
    for (event, range) in definitions.parse(&text, source.len()) {
        builder.handle(event, range);
        // Stop once the block is complete. Don't cut off by source
        // position: the parser can place events just past the block's
        // text, e.g. empty cells padding a short table row.
        if builder.is_complete() {
            break;
        }
    }
    builder.finish()
}

/// A container being built while walking parser events.
#[derive(Debug)]
enum Frame {
    Root(Vec<RenderedBlock>),
    Paragraph(InlineBuilder),
    Heading(u8, InlineBuilder),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item {
        task: Option<Task>,
        children: Vec<RenderedBlock>,
        /// Text directly in a tight list item, which has no paragraph tags.
        inline: InlineBuilder,
    },
    /// A blockquote, with its alert kind if it's a GitHub alert.
    BlockQuote(Option<AlertKind>, Vec<RenderedBlock>),
    CodeBlock {
        language: Option<String>,
        code: InlineBuilder,
    },
    Html(String),
    Table {
        table: Table,
        row: Vec<InlineText>,
    },
    Cell(InlineBuilder),
    /// A footnote's text, and its label.
    Footnote(String, Vec<RenderedBlock>),
}

#[derive(Debug)]
struct TreeBuilder<'a> {
    stack: Vec<Frame>,
    /// For footnotes' numbers.
    definitions: &'a Definitions,
}

impl<'a> TreeBuilder<'a> {
    fn new(definitions: &'a Definitions) -> Self {
        Self {
            stack: vec![Frame::Root(Vec::new())],
            definitions,
        }
    }

    fn handle(&mut self, event: Event, range: std::ops::Range<usize>) {
        // Inline events go to the innermost inline container.
        match self.stack.last_mut() {
            Some(
                Frame::Paragraph(inline)
                | Frame::Heading(_, inline)
                | Frame::Cell(inline)
                | Frame::Item { inline, .. },
            ) => {
                if let Event::FootnoteReference(label) = &event {
                    if let Some(number) = self.definitions.footnote_number(label) {
                        inline.footnote_reference(label, number, range);
                    }
                    return;
                }
                if inline.handle(&event, range.clone()) {
                    return;
                }
            }
            Some(Frame::CodeBlock { code, .. }) => {
                if let Event::Text(_) = &event {
                    code.handle(&event, range);
                    return;
                }
            }
            Some(Frame::Html(html)) => {
                if let Event::Html(text) = &event {
                    html.push_str(text);
                    return;
                }
            }
            _ => {}
        }

        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Rule => self.push_block(RenderedBlock::ThematicBreak),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item { task, .. }) = self.stack.last_mut() {
                    *task = Some(Task {
                        checked,
                        marker: range.start,
                    });
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        let frame = match tag {
            Tag::Paragraph => Frame::Paragraph(InlineBuilder::default()),
            Tag::Heading { level, .. } => {
                Frame::Heading(heading_level(level), InlineBuilder::default())
            }
            Tag::List(start) => Frame::List {
                start,
                items: Vec::new(),
            },
            Tag::Item => Frame::Item {
                task: None,
                children: Vec::new(),
                inline: InlineBuilder::default(),
            },
            Tag::BlockQuote(kind) => Frame::BlockQuote(kind.map(AlertKind::from), Vec::new()),
            Tag::CodeBlock(kind) => Frame::CodeBlock {
                language: match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .map(|language| language.to_owned()),
                    CodeBlockKind::Indented => None,
                },
                code: InlineBuilder::default(),
            },
            Tag::HtmlBlock => Frame::Html(String::new()),
            Tag::Table(alignments) => Frame::Table {
                table: Table {
                    alignments: alignments.into_iter().map(column_alignment).collect(),
                    header: Vec::new(),
                    rows: Vec::new(),
                },
                row: Vec::new(),
            },
            Tag::TableCell => Frame::Cell(InlineBuilder::default()),
            Tag::FootnoteDefinition(label) => Frame::Footnote(label.to_string(), Vec::new()),
            // Table heads and rows are tracked in the table frame; other tags
            // are inline (handled above) or extensions Kertas doesn't enable.
            _ => return,
        };
        // A nested block in a tight list item ends the item's leading text.
        self.flush_item_text();
        self.stack.push(frame);
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::TableHead => {
                if let Some(Frame::Table { table, row }) = self.stack.last_mut() {
                    table.header = std::mem::take(row);
                }
                return;
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table { table, row }) = self.stack.last_mut() {
                    table.rows.push(std::mem::take(row));
                }
                return;
            }
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::Table
            | TagEnd::TableCell
            | TagEnd::FootnoteDefinition => {}
            _ => return,
        }

        let Some(frame) = self.stack.pop() else {
            return;
        };
        match frame {
            Frame::Paragraph(inline) => self.push_block(RenderedBlock::Paragraph(inline.finish())),
            Frame::Heading(level, inline) => self.push_block(RenderedBlock::Heading {
                level,
                text: inline.finish(),
            }),
            Frame::List { start, items } => self.push_block(RenderedBlock::List { start, items }),
            Frame::Item {
                task,
                mut children,
                inline,
            } => {
                if !inline.is_empty() {
                    children.push(RenderedBlock::Paragraph(inline.finish()));
                }
                if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                    items.push(ListItem { task, children });
                }
            }
            Frame::BlockQuote(None, children) => {
                self.push_block(RenderedBlock::BlockQuote(children))
            }
            Frame::BlockQuote(Some(kind), children) => {
                self.push_block(RenderedBlock::Alert { kind, children })
            }
            Frame::CodeBlock { language, code } => self.push_block(RenderedBlock::CodeBlock {
                language,
                code: code.finish_code(),
            }),
            Frame::Html(html) => self.push_block(RenderedBlock::Html(html)),
            Frame::Table { table, .. } => self.push_block(RenderedBlock::Table(table)),
            Frame::Cell(inline) => {
                if let Some(Frame::Table { row, .. }) = self.stack.last_mut() {
                    row.push(inline.finish());
                }
            }
            Frame::Footnote(label, children) => {
                self.push_block(RenderedBlock::FootnoteDefinition { label, children })
            }
            Frame::Root(_) => unreachable!("root frame is never closed"),
        }
    }

    /// Adds a finished block to the innermost container.
    fn push_block(&mut self, block: RenderedBlock) {
        self.flush_item_text();
        if let Some(
            Frame::Root(children)
            | Frame::BlockQuote(_, children)
            | Frame::Item { children, .. }
            | Frame::Footnote(_, children),
        ) = self.stack.last_mut()
        {
            children.push(block);
        }
    }

    /// Moves a tight list item's pending text into a paragraph, so it stays
    /// ahead of the nested block that follows it.
    fn flush_item_text(&mut self) {
        if let Some(Frame::Item {
            children, inline, ..
        }) = self.stack.last_mut()
            && !inline.is_empty()
        {
            let text = std::mem::take(inline).finish();
            children.push(RenderedBlock::Paragraph(text));
        }
    }

    /// Whether a complete top-level block has been built.
    fn is_complete(&self) -> bool {
        matches!(self.stack.as_slice(), [Frame::Root(children)] if !children.is_empty())
    }

    fn finish(mut self) -> Option<RenderedBlock> {
        match self.stack.drain(..1).next() {
            Some(Frame::Root(children)) => children.into_iter().next(),
            _ => None,
        }
    }
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

fn column_alignment(alignment: Alignment) -> ColumnAlignment {
    match alignment {
        Alignment::None => ColumnAlignment::None,
        Alignment::Left => ColumnAlignment::Left,
        Alignment::Center => ColumnAlignment::Center,
        Alignment::Right => ColumnAlignment::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(text: &str) -> Vec<RenderedBlock> {
        render_blocks(text, &Blocks::parse(text))
    }

    fn render_one(text: &str) -> RenderedBlock {
        let mut blocks = render(text);
        assert_eq!(blocks.len(), 1, "expected one block: {blocks:?}");
        blocks.remove(0)
    }

    /// A compact description of a block tree, for readable assertions.
    fn describe(block: &RenderedBlock) -> String {
        match block {
            RenderedBlock::Paragraph(text) => format!("p({})", text.text),
            RenderedBlock::Heading { level, text } => format!("h{level}({})", text.text),
            RenderedBlock::ThematicBreak => "hr".into(),
            RenderedBlock::List { start, items } => {
                let items: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let marker = match item.task.map(|task| task.checked) {
                            Some(true) => "[x] ",
                            Some(false) => "[ ] ",
                            None => "",
                        };
                        let children: Vec<String> = item.children.iter().map(describe).collect();
                        format!("{marker}{}", children.join(" "))
                    })
                    .collect();
                match start {
                    Some(start) => format!("ol{start}[{}]", items.join(", ")),
                    None => format!("ul[{}]", items.join(", ")),
                }
            }
            RenderedBlock::BlockQuote(children) => {
                let children: Vec<String> = children.iter().map(describe).collect();
                format!("quote[{}]", children.join(" "))
            }
            RenderedBlock::Alert { kind, children } => {
                let children: Vec<String> = children.iter().map(describe).collect();
                format!("{}[{}]", kind.title().to_lowercase(), children.join(" "))
            }
            RenderedBlock::CodeBlock { language, code } => {
                format!(
                    "code:{}({:?})",
                    language.as_deref().unwrap_or(""),
                    code.text
                )
            }
            RenderedBlock::Table(table) => {
                let row = |cells: &[InlineText]| {
                    cells
                        .iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                };
                let rows: Vec<String> = table.rows.iter().map(|cells| row(cells)).collect();
                format!("table[{} / {}]", row(&table.header), rows.join(" / "))
            }
            RenderedBlock::Html(html) => format!("html({html:?})"),
            RenderedBlock::Raw(source) => format!("raw({source})"),
            RenderedBlock::FootnoteDefinition { label, children } => {
                let children: Vec<String> = children.iter().map(describe).collect();
                format!("note:{label}[{}]", children.join(" "))
            }
        }
    }

    #[track_caller]
    fn assert_renders(text: &str, expected: &[&str]) {
        let described: Vec<String> = render(text).iter().map(describe).collect();
        assert_eq!(described, expected);
    }

    #[test]
    fn text_blocks() {
        assert_renders(
            "# Title\n\nSome *text*.\n\n***\n",
            &["h1(Title)", "p(Some text.)", "hr"],
        );
    }

    #[test]
    fn tight_bullet_list() {
        assert_renders("- one\n- **two**\n", &["ul[p(one), p(two)]"]);
    }

    #[test]
    fn loose_list() {
        assert_renders("- one\n\n- two\n", &["ul[p(one), p(two)]"]);
    }

    #[test]
    fn ordered_list_keeps_start_number() {
        assert_renders("3. three\n4. four\n", &["ol3[p(three), p(four)]"]);
    }

    #[test]
    fn nested_lists() {
        assert_renders(
            "- a\n  - b\n    1. c\n- d\n",
            &["ul[p(a) ul[p(b) ol1[p(c)]], p(d)]"],
        );
    }

    #[test]
    fn task_list() {
        assert_renders(
            "- [ ] todo\n- [x] done\n- plain\n",
            &["ul[[ ] p(todo), [x] p(done), p(plain)]"],
        );
    }

    #[test]
    fn task_markers_point_at_their_source() {
        let source = "- [ ] todo\n  - [X] nested\n";
        let RenderedBlock::List { items, .. } = render_one(source) else {
            panic!("expected a list");
        };
        let outer = items[0].task.expect("a task");
        assert_eq!(
            (outer.checked, &source[outer.marker..outer.marker + 3]),
            (false, "[ ]")
        );
        let RenderedBlock::List { items: nested, .. } = &items[0].children[1] else {
            panic!("expected a nested list: {:?}", items[0].children);
        };
        let inner = nested[0].task.expect("a task");
        assert_eq!(
            (inner.checked, &source[inner.marker..inner.marker + 3]),
            (true, "[X]")
        );
    }

    #[test]
    fn list_item_with_multiple_blocks() {
        assert_renders(
            "- para one\n\n  para two\n\n  ```\n  code\n  ```\n",
            &[r#"ul[p(para one) p(para two) code:("code\n")]"#],
        );
    }

    #[test]
    fn blockquote_with_nested_content() {
        assert_renders(
            "> # Title\n> text\n>\n> > nested\n>\n> - item\n",
            &["quote[h1(Title) p(text) quote[p(nested)] ul[p(item)]]"],
        );
    }

    #[test]
    fn github_alerts() {
        assert_renders(
            "> [!NOTE]\n> Useful.\n\n> [!tip]\n> - one\n\n> [!IMPORTANT]\n> a\n\n> [!WARNING]\n> b\n\n> [!CAUTION]\n> c\n",
            &[
                "note[p(Useful.)]",
                "tip[ul[p(one)]]",
                "important[p(a)]",
                "warning[p(b)]",
                "caution[p(c)]",
            ],
        );
    }

    #[test]
    fn alert_text_maps_to_source() {
        let text = "> [!NOTE]\n> Useful.\n";
        let RenderedBlock::Alert { children, .. } = render_one(text) else {
            panic!()
        };
        let RenderedBlock::Paragraph(inline) = &children[0] else {
            panic!()
        };
        assert_eq!(&text[inline.segments[0].source.clone()], "Useful.");
    }

    #[test]
    fn unknown_or_misplaced_alert_markers_stay_blockquotes() {
        assert_renders(
            "> [!FOO]\n> text\n\n> text\n> [!NOTE]\n\n> [!NOTE] same line\n",
            &[
                "quote[p([!FOO] text)]",
                "quote[p(text [!NOTE])]",
                "quote[p([!NOTE] same line)]",
            ],
        );
    }

    #[test]
    fn lazy_blockquote_continuation() {
        assert_renders("> one\ntwo\n", &["quote[p(one two)]"]);
    }

    #[test]
    fn fenced_code_block() {
        assert_renders(
            "```rust title=x\nfn main() {}\n\n```\n",
            &[r#"code:rust("fn main() {}\n\n")"#],
        );
    }

    #[test]
    fn code_block_keeps_markdown_literal() {
        assert_renders("```\n**not bold**\n```", &[r#"code:("**not bold**\n")"#]);
    }

    #[test]
    fn code_maps_to_source() {
        let text = "```rust\nlet a;\nlet b;\n```";
        let RenderedBlock::CodeBlock { code, .. } = render_one(text) else {
            panic!()
        };
        assert_eq!(code.text, "let a;\nlet b;\n");
        assert_eq!(
            code.source_offset(0),
            8,
            "first line starts after the fence"
        );
        assert_eq!(code.source_offset(7), 15);
    }

    #[test]
    fn indented_code_block() {
        assert_renders(
            "Para\n\n    code\n      indented\n",
            &["p(Para)", r#"code:("code\n  indented")"#],
        );
    }

    #[test]
    fn table_with_alignment() {
        let block = render_one("| a | **b** | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n| 4 |\n");
        // Short rows are padded with empty cells.
        assert_eq!(describe(&block), "table[a|b|c / 1|2|3 / 4||]");
        let RenderedBlock::Table(table) = block else {
            unreachable!()
        };
        assert_eq!(
            table.alignments,
            [
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Right
            ]
        );
        assert!(table.header[1].segments[0].style.strong);
    }

    #[test]
    fn showcase_alignment_table() {
        // Regression test: this table from the showcase rendered as raw source.
        let text = "## Tables\n\n| Left | Center | Right |\n|:-----|:------:|------:|\n| a    | **b**  | `c`   |\n| longer cell text | 2 | 3.00 |\n| short row |\n\n## HTML\n";
        assert_renders(
            text,
            &[
                "h2(Tables)",
                "table[Left|Center|Right / a|b|c / longer cell text|2|3.00 / short row||]",
                "h2(HTML)",
            ],
        );
    }

    #[test]
    fn showcase_renders_everything_except_definitions() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/showcase.md"
        ))
        .unwrap();
        let raw: Vec<String> = render(&text)
            .iter()
            .filter(|block| matches!(block, RenderedBlock::Raw(_)))
            .map(describe)
            .collect();
        assert_eq!(raw, ["raw([docs]: https://example.com/docs)"]);
    }

    #[test]
    fn html_block_is_kept_as_source() {
        assert_renders(
            "<div>\n<script>alert(1)</script>\n</div>\n",
            &[r#"html("<div>\n<script>alert(1)</script>\n</div>")"#],
        );
    }

    #[test]
    fn front_matter_and_definitions_are_raw() {
        assert_renders(
            "---\nk: v\n---\n\nSee [kertas][k].\n\n[k]: https://example.com\n",
            &[
                "raw(---\nk: v\n---)",
                "p(See kertas.)",
                "raw([k]: https://example.com)",
            ],
        );
    }

    #[test]
    fn reference_links_resolve_inside_lists_and_tables() {
        let text = "- [kertas][k]\n\n| [kertas][k] |\n|---|\n\n[k]: /x\n";
        let blocks = render(text);
        let RenderedBlock::List { items, .. } = &blocks[0] else {
            panic!("expected list: {blocks:?}")
        };
        let RenderedBlock::Paragraph(text) = &items[0].children[0] else {
            panic!()
        };
        assert!(text.segments[0].style.link);
        let RenderedBlock::Table(table) = &blocks[1] else {
            panic!()
        };
        assert!(table.header[0].segments[0].style.link);
    }

    #[test]
    fn inline_source_ranges_are_relative_to_block() {
        let text = "Intro\n\n- **bold**\n";
        let blocks = render(text);
        let RenderedBlock::List { items, .. } = &blocks[1] else {
            panic!()
        };
        let RenderedBlock::Paragraph(inline) = &items[0].children[0] else {
            panic!()
        };
        // The list's source is "- **bold**"; "bold" is at 4..8.
        assert_eq!(inline.segments[0].source, 4..8);
    }

    #[test]
    fn large_document_renders_quickly() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/10k.md"
        ))
        .unwrap();
        let start = std::time::Instant::now();
        let blocks = Blocks::parse(&text);
        let rendered = render_blocks(&text, &blocks);
        let elapsed = start.elapsed();
        assert_eq!(rendered.len(), blocks.len());
        assert!(
            !rendered
                .iter()
                .any(|block| matches!(block, RenderedBlock::Raw(_))),
            "every block in the fixture should be rendered"
        );
        // Typically 34 ms in a debug build and 6 ms in release (M2,
        // 2026-10-13); N1 is 500 ms end to end. The bound leaves room for a
        // slow or busy machine.
        assert!(elapsed.as_millis() < 150, "took {elapsed:?}");
        eprintln!("parsed and rendered {} blocks in {elapsed:?}", blocks.len());
    }

    #[test]
    fn many_link_definitions_render_quickly() {
        // Rendering mustn't depend on how many definitions there are: each
        // block once took time proportional to all of them.
        let mut text = String::new();
        for index in 0..2000 {
            text.push_str(&format!(
                "Paragraph {index} with a [link][d{}].\n\n",
                index % 500
            ));
        }
        for index in 0..500 {
            text.push_str(&format!("[d{index}]: https://example.com/{index}\n"));
        }
        let start = std::time::Instant::now();
        let blocks = Blocks::parse(&text);
        let rendered = render_blocks(&text, &blocks);
        let elapsed = start.elapsed();
        assert_eq!(rendered.len(), 2500);
        let RenderedBlock::Paragraph(first) = &rendered[0] else {
            panic!("{:?}", rendered[0]);
        };
        assert!(first.segments.iter().any(|segment| segment.style.link));
        // Typically 10 ms in a debug build (M2, 2026-10-13); it was 2 s
        // before definitions were looked up instead of re-parsed.
        assert!(elapsed.as_millis() < 100, "took {elapsed:?}");
    }

    // Edge cases (#38).

    #[test]
    fn setext_underline_right_after_a_paragraph_line() {
        // The underline turns the whole paragraph into the heading, its
        // line break a space like any soft break.
        let blocks = render("Para\nHeading\n===\n");
        let [RenderedBlock::Heading { level: 1, text }] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert_eq!(text.text, "Para Heading");
    }

    #[test]
    fn footnote_definitions_keep_their_blocks() {
        assert_renders(
            "Text[^n].\n\n[^n]: First.\n\n    - item\n",
            &["p(Text¹.)", "note:n[p(First.) ul[p(item)]]"],
        );
    }

    #[test]
    fn footnote_references_inside_lists_quotes_and_tables() {
        assert_renders(
            "- a[^x]\n\n> b[^y]\n\n| c[^x] |\n|---|\n\n[^x]: X.\n\n[^y]: Y.\n",
            &[
                "ul[p(a¹)]",
                "quote[p(b²)]",
                "table[c¹ / ]",
                "note:x[p(X.)]",
                "note:y[p(Y.)]",
            ],
        );
    }

    #[test]
    fn references_without_a_definition_stay_text() {
        assert_renders("A[^missing] and [^].\n", &["p(A[^missing] and [^].)"]);
    }

    #[test]
    fn code_never_has_footnotes() {
        assert_renders(
            "`x[^n]`\n\n```\ny[^n]\n```\n\n[^n]: N.\n\n```\nunclosed[^n]",
            &[
                "p(x[^n])",
                r#"code:("y[^n]\n")"#,
                "note:n[p(N.)]",
                r#"code:("unclosed[^n]")"#,
            ],
        );
    }
}
