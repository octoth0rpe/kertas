//! A document laid out for printing (#17): its text, with styled runs and
//! paragraphs, ready to turn into a native styled-text document on each
//! platform. See docs/planning/spike-17-print.md.
//!
//! The layout always uses light colors (black on white), whatever the
//! app's theme. Hard line breaks within a paragraph, and the lines of code
//! and HTML blocks, are U+2028 (line separator), so each block stays one
//! paragraph.

use std::ops::Range;

use crate::inline::InlineText;
use crate::model::Model;
use crate::rendered::{AlertKind, ColumnAlignment, RenderedBlock};
use crate::settings::ListIndent;
use crate::tabs::expand;

/// A line break within a paragraph.
pub const LINE_SEPARATOR: char = '\u{2028}';

/// How far each quote level, and each list level with the standard
/// `list_indent`, is indented, in points.
const INDENT: f64 = 16.;

/// Space after each block, in points.
const BLOCK_SPACING: f64 = 7.;

/// The text to print, with its styles.
#[derive(Debug, Default)]
pub struct PrintLayout {
    pub text: String,
    /// Styled runs covering the whole text, in order.
    pub runs: Vec<Run>,
    /// Paragraphs, each ending with a newline, in order.
    pub paragraphs: Vec<Paragraph>,
    /// The tables that `Frame::Cell` paragraphs belong to.
    pub tables: Vec<TableLayout>,
    /// How many spaces a tab in code is printed as.
    tab_width: u32,
    /// How far each list level is indented, in points.
    list_indent: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Byte range in `PrintLayout::text`.
    pub range: Range<usize>,
    pub style: RunStyle,
}

/// How a run of text looks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RunStyle {
    /// The heading level, if this is heading text.
    pub heading: Option<u8>,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    /// Monospace, with a grey background unless it's in a code block.
    pub code: bool,
    pub link: bool,
    /// Grey, for quotes and source shown as-is.
    pub muted: bool,
    /// In an alert's color, for its title.
    pub alert: Option<AlertKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    /// Byte range in `PrintLayout::text`, including its newline.
    pub range: Range<usize>,
    /// Indentation, in points.
    pub indent: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub alignment: Alignment,
    pub frame: Frame,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Natural,
    Center,
    Right,
}

/// A box drawn around or under a paragraph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Frame {
    #[default]
    None,
    /// A code block's grey box.
    Code,
    /// A thematic break: a line across the page.
    Rule,
    /// A table cell.
    Cell {
        table: usize,
        row: usize,
        column: usize,
        header: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLayout {
    pub columns: usize,
}

impl PrintLayout {
    /// Lays out every block in `model` for printing, with each tab in
    /// code shown as `tab_width` spaces, and lists indented by
    /// `list_indent`.
    pub fn new(model: &Model, tab_width: u32, list_indent: ListIndent) -> Self {
        let mut layout = Self {
            tab_width,
            list_indent: INDENT * f64::from(list_indent.scale()),
            ..Self::default()
        };
        for item in &model.items {
            layout.block(&item.rendered, 0., RunStyle::default());
        }
        layout
    }

    fn push(&mut self, text: &str, style: RunStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        // Line breaks inside a block stay inside its paragraph.
        self.text
            .push_str(&text.replace('\n', &LINE_SEPARATOR.to_string()));
        self.runs.push(Run {
            range: start..self.text.len(),
            style,
        });
    }

    fn inline(&mut self, text: &InlineText, base: RunStyle) {
        let mut position = 0;
        for segment in &text.segments {
            if segment.display.start > position {
                self.push(&text.text[position..segment.display.start], base);
            }
            let style = segment.style;
            self.push(
                &text.text[segment.display.clone()],
                RunStyle {
                    bold: base.bold || style.strong,
                    italic: base.italic || style.emphasis,
                    strikethrough: style.strikethrough,
                    code: base.code || style.code,
                    link: style.link,
                    ..base
                },
            );
            position = segment.display.end;
        }
        if position < text.text.len() {
            self.push(&text.text[position..], base);
        }
    }

    /// Ends the paragraph that started at `start`.
    fn end_paragraph(&mut self, start: usize, paragraph: Paragraph) {
        let newline = self.text.len();
        self.text.push('\n');
        self.runs.push(Run {
            range: newline..self.text.len(),
            style: RunStyle::default(),
        });
        self.paragraphs.push(Paragraph {
            range: start..self.text.len(),
            ..paragraph
        });
    }

    fn block(&mut self, block: &RenderedBlock, indent: f64, style: RunStyle) {
        let start = self.text.len();
        let paragraph = Paragraph {
            range: 0..0,
            indent,
            space_before: 0.,
            space_after: BLOCK_SPACING,
            alignment: Alignment::Natural,
            frame: Frame::None,
        };
        match block {
            RenderedBlock::Paragraph(text) => {
                self.inline(text, style);
                self.end_paragraph(start, paragraph);
            }
            RenderedBlock::Heading { level, text } => {
                self.inline(
                    text,
                    RunStyle {
                        heading: Some(*level),
                        bold: true,
                        ..style
                    },
                );
                self.end_paragraph(
                    start,
                    Paragraph {
                        space_before: 10.,
                        ..paragraph
                    },
                );
            }
            RenderedBlock::ThematicBreak => {
                // The rule is drawn by the frame; the paragraph needs some
                // text to have a line.
                self.push(" ", style);
                self.end_paragraph(
                    start,
                    Paragraph {
                        frame: Frame::Rule,
                        ..paragraph
                    },
                );
            }
            RenderedBlock::List {
                start: first,
                items,
            } => {
                for (number, item) in items.iter().enumerate() {
                    let marker = match (item.task.map(|task| task.checked), first) {
                        (Some(true), _) => "☑".to_owned(),
                        (Some(false), _) => "☐".to_owned(),
                        (None, Some(first)) => format!("{}.", first + number as u64),
                        (None, None) => "•".to_owned(),
                    };
                    for (index, child) in item.children.iter().enumerate() {
                        if index == 0 {
                            // The marker starts the item's first paragraph.
                            let item_start = self.text.len();
                            self.push(&format!("{marker}  "), style);
                            let first_paragraph = self.paragraphs.len();
                            self.block(child, indent + self.list_indent, style);
                            if let Some(paragraph) = self.paragraphs.get_mut(first_paragraph) {
                                paragraph.range.start = item_start;
                            }
                        } else {
                            self.block(child, indent + self.list_indent, style);
                        }
                    }
                }
            }
            RenderedBlock::Alert { kind, children } => {
                self.push(
                    kind.title(),
                    RunStyle {
                        bold: true,
                        alert: Some(*kind),
                        ..style
                    },
                );
                self.end_paragraph(start, paragraph);
                for child in children {
                    self.block(child, indent + INDENT, style);
                }
            }
            RenderedBlock::BlockQuote(children) => {
                for child in children {
                    self.block(
                        child,
                        indent + INDENT,
                        RunStyle {
                            muted: true,
                            ..style
                        },
                    );
                }
            }
            RenderedBlock::CodeBlock { code, .. } => {
                self.push(
                    &expand(code.text.trim_end_matches('\n'), self.tab_width),
                    RunStyle {
                        code: true,
                        ..RunStyle::default()
                    },
                );
                self.end_paragraph(
                    start,
                    Paragraph {
                        frame: Frame::Code,
                        ..paragraph
                    },
                );
            }
            RenderedBlock::Table(table) => {
                let columns = table.header.len().max(1);
                let index = self.tables.len();
                self.tables.push(TableLayout { columns });
                let rows = std::iter::once(&table.header).chain(&table.rows);
                for (row, cells) in rows.enumerate() {
                    for column in 0..columns {
                        let cell_start = self.text.len();
                        if let Some(text) = cells.get(column) {
                            self.inline(
                                text,
                                RunStyle {
                                    bold: row == 0,
                                    ..style
                                },
                            );
                        }
                        let alignment = match table.alignments.get(column) {
                            Some(ColumnAlignment::Center) => Alignment::Center,
                            Some(ColumnAlignment::Right) => Alignment::Right,
                            _ => Alignment::Natural,
                        };
                        self.end_paragraph(
                            cell_start,
                            Paragraph {
                                space_after: 0.,
                                alignment,
                                frame: Frame::Cell {
                                    table: index,
                                    row,
                                    column,
                                    header: row == 0,
                                },
                                ..paragraph.clone()
                            },
                        );
                    }
                }
                // Space after the table.
                let after = self.text.len();
                self.end_paragraph(after, paragraph);
            }
            RenderedBlock::Html(source) | RenderedBlock::Raw(source) => {
                self.push(
                    &expand(source.trim_end(), self.tab_width),
                    RunStyle {
                        code: true,
                        muted: true,
                        ..RunStyle::default()
                    },
                );
                self.end_paragraph(
                    start,
                    Paragraph {
                        frame: Frame::Code,
                        ..paragraph
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(text: &str) -> PrintLayout {
        PrintLayout::new(&Model::new(text), 4, ListIndent::Standard)
    }

    /// The text of each paragraph, without its newline.
    fn paragraphs(layout: &PrintLayout) -> Vec<&str> {
        layout
            .paragraphs
            .iter()
            .map(|paragraph| layout.text[paragraph.range.clone()].trim_end_matches('\n'))
            .collect()
    }

    fn style_of<'a>(layout: &'a PrintLayout, text: &str) -> &'a RunStyle {
        &layout
            .runs
            .iter()
            .find(|run| &layout.text[run.range.clone()] == text)
            .unwrap_or_else(|| panic!("no run {text:?}"))
            .style
    }

    #[test]
    fn runs_and_paragraphs_cover_the_text() {
        let layout = layout(
            "# Title\n\nSome **bold** text\\\nand more.\n\n- a\n- b\n\n```\ncode\n```\n\n| x | y |\n|---|--:|\n| 1 | 2 |\n\n---\n",
        );
        let mut end = 0;
        for run in &layout.runs {
            assert_eq!(run.range.start, end);
            end = run.range.end;
        }
        assert_eq!(end, layout.text.len());
        let mut end = 0;
        for paragraph in &layout.paragraphs {
            assert_eq!(paragraph.range.start, end);
            assert!(layout.text[paragraph.range.clone()].ends_with('\n'));
            end = paragraph.range.end;
        }
        assert_eq!(end, layout.text.len());
    }

    #[test]
    fn headings_and_inline_styles() {
        let layout = layout("## Title\n\nSome **bold**, *italic*, ~~gone~~ and `code`.\n");
        assert_eq!(style_of(&layout, "Title").heading, Some(2));
        assert!(style_of(&layout, "Title").bold);
        assert!(style_of(&layout, "bold").bold);
        assert!(style_of(&layout, "italic").italic);
        assert!(style_of(&layout, "gone").strikethrough);
        assert!(style_of(&layout, "code").code);
    }

    #[test]
    fn alerts_have_a_title_and_indented_text() {
        let layout = layout("> [!WARNING]\n> Careful.\n");
        assert_eq!(paragraphs(&layout), ["Warning", "Careful."]);
        let title = style_of(&layout, "Warning");
        assert_eq!(title.alert, Some(AlertKind::Warning));
        assert!(title.bold);
        let text = style_of(&layout, "Careful.");
        assert!(!text.muted && text.alert.is_none());
        assert_eq!(
            (layout.paragraphs[0].indent, layout.paragraphs[1].indent),
            (0., INDENT)
        );
    }

    #[test]
    fn hard_breaks_stay_in_the_paragraph() {
        let layout = layout("one\\\ntwo\n");
        assert_eq!(paragraphs(&layout), ["one\u{2028}two"]);
    }

    #[test]
    fn list_markers() {
        let layout = layout("- a\n- b\n\n3. c\n4. d\n\n- [ ] e\n- [x] f\n");
        assert_eq!(
            paragraphs(&layout),
            ["•  a", "•  b", "3.  c", "4.  d", "☐  e", "☑  f"]
        );
        assert_eq!(layout.paragraphs[0].indent, INDENT);
    }

    #[test]
    fn list_indent_scales_lists_but_not_quotes() {
        let text = "- a\n  - b\n\n> c\n";
        let indents = |indent| {
            PrintLayout::new(&Model::new(text), 4, indent)
                .paragraphs
                .iter()
                .map(|paragraph| paragraph.indent)
                .collect::<Vec<_>>()
        };
        assert_eq!(indents(ListIndent::Standard), [16., 32., 16.]);
        assert_eq!(indents(ListIndent::Compact), [12., 24., 16.]);
        assert_eq!(indents(ListIndent::Wide), [24., 48., 16.]);
    }

    #[test]
    fn code_tabs_are_expanded() {
        let layout = PrintLayout::new(&Model::new("```\n\tx\n```\n"), 2, ListIndent::Standard);
        assert_eq!(layout.text.trim_end(), "  x");
    }

    #[test]
    fn code_is_one_framed_paragraph() {
        let layout = layout("```\nfn a() {\n    b\n}\n```\n");
        assert_eq!(paragraphs(&layout), ["fn a() {\u{2028}    b\u{2028}}"]);
        assert_eq!(layout.paragraphs[0].frame, Frame::Code);
    }

    #[test]
    fn tables_have_cells_with_alignment() {
        let layout = layout("| a | b | c |\n|---|:-:|--:|\n| 1 | 2 | 3 |\n");
        assert_eq!(layout.tables, [TableLayout { columns: 3 }]);
        let cells: Vec<_> = layout
            .paragraphs
            .iter()
            .filter_map(|paragraph| match paragraph.frame {
                Frame::Cell {
                    row,
                    column,
                    header,
                    ..
                } => Some((row, column, header, paragraph.alignment)),
                _ => None,
            })
            .collect();
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[0], (0, 0, true, Alignment::Natural));
        assert_eq!(cells[4], (1, 1, false, Alignment::Center));
        assert_eq!(cells[5], (1, 2, false, Alignment::Right));
    }

    #[test]
    fn rules_and_quotes() {
        let layout = layout("> quoted\n\n---\n");
        assert!(style_of(&layout, "quoted").muted);
        assert_eq!(layout.paragraphs[0].indent, INDENT);
        assert_eq!(layout.paragraphs[1].frame, Frame::Rule);
    }

    // Edge cases (#38).

    #[test]
    fn extra_cells_in_a_row_are_dropped() {
        // GFM ignores cells beyond the header's columns.
        let layout = layout("| a | b |\n|---|---|\n| 1 | 2 | 3 |\n");
        let cells = layout
            .paragraphs
            .iter()
            .filter(|paragraph| matches!(paragraph.frame, Frame::Cell { .. }))
            .count();
        assert_eq!(cells, 4);
        assert!(!layout.text.contains('3'));
    }

    #[test]
    fn a_header_only_table() {
        let layout = layout("| a | b |\n|---|---|\n");
        assert_eq!(layout.tables, [TableLayout { columns: 2 }]);
    }

    #[test]
    fn task_items_in_ordered_lists_show_a_box() {
        let layout = layout("1. [ ] a\n2. [x] b\n");
        assert_eq!(paragraphs(&layout), ["☐  a", "☑  b"]);
    }

    #[test]
    fn headings_in_quotes_are_muted_headings() {
        let layout = layout("> # Quoted\n");
        let style = style_of(&layout, "Quoted");
        assert_eq!(style.heading, Some(1));
        assert!(style.muted);
    }
}
