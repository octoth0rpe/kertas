//! How a revealed block's source is styled (C4).
//!
//! A revealed block shows its Markdown source. Prose blocks use the same
//! fonts and inline styles as when rendered, with the syntax characters
//! dimmed. Blocks whose alignment matters (code, tables, HTML, front
//! matter) are shown in monospace. See `docs/planning/caret-and-reveal.md`.

use std::ops::Range;

use crate::blocks::BlockKind;
use crate::inline::{InlineStyle, InlineText};
use crate::rendered::{RenderedBlock, render_block};

/// The overall look of a revealed block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealFont {
    /// Body font, or a heading's size for `Some(level)`.
    Prose {
        heading: Option<u8>,
    },
    Monospace,
}

/// How one range of source is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStyle {
    /// Text that's visible when rendered, with its inline formatting.
    Content(InlineStyle),
    /// Markdown syntax that's hidden when rendered, drawn dimmed.
    Syntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    /// Byte range in the block's source.
    pub range: Range<usize>,
    pub style: SpanStyle,
}

/// A revealed block's source with styles covering all of it, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSource {
    pub font: RevealFont,
    pub spans: Vec<SourceSpan>,
}

/// Styles the source of a top-level block for display while revealed.
pub fn styled_source(source: &str, kind: BlockKind, definitions: &str) -> StyledSource {
    let font = match kind {
        BlockKind::Heading { level } => RevealFont::Prose {
            heading: Some(level),
        },
        BlockKind::Paragraph
        | BlockKind::List { .. }
        | BlockKind::BlockQuote
        | BlockKind::ThematicBreak => RevealFont::Prose { heading: None },
        BlockKind::CodeBlock { .. }
        | BlockKind::Table
        | BlockKind::HtmlBlock
        | BlockKind::FrontMatter
        | BlockKind::LinkReferenceDefinition => RevealFont::Monospace,
    };

    let spans = match font {
        RevealFont::Prose { .. } => {
            let mut content = Vec::new();
            if let Some(block) = render_block(source, definitions) {
                collect_segments(&block, &mut content);
            }
            fill_gaps(source.len(), content)
        }
        RevealFont::Monospace if source.is_empty() => Vec::new(),
        RevealFont::Monospace => vec![SourceSpan {
            range: 0..source.len(),
            style: SpanStyle::Content(InlineStyle::default()),
        }],
    };
    StyledSource { font, spans }
}

/// Collects the content segments of every inline text in a block tree.
fn collect_segments(block: &RenderedBlock, out: &mut Vec<SourceSpan>) {
    let mut add = |text: &InlineText| {
        out.extend(text.segments.iter().map(|segment| SourceSpan {
            range: segment.source.clone(),
            style: SpanStyle::Content(segment.style),
        }));
    };
    match block {
        RenderedBlock::Paragraph(text) | RenderedBlock::Heading { text, .. } => add(text),
        RenderedBlock::List { items, .. } => {
            for item in items {
                for child in &item.children {
                    collect_segments(child, out);
                }
            }
        }
        RenderedBlock::BlockQuote(children) => {
            for child in children {
                collect_segments(child, out);
            }
        }
        RenderedBlock::CodeBlock { code, .. } => add(code),
        RenderedBlock::Table(table) => {
            for cell in table.header.iter().chain(table.rows.iter().flatten()) {
                add(cell);
            }
        }
        RenderedBlock::ThematicBreak | RenderedBlock::Html(_) | RenderedBlock::Raw(_) => {}
    }
}

/// Sorts content spans and fills the gaps between them with syntax.
fn fill_gaps(len: usize, mut content: Vec<SourceSpan>) -> Vec<SourceSpan> {
    content.sort_by_key(|span| span.range.start);
    let mut spans = Vec::with_capacity(content.len() * 2 + 1);
    let mut position = 0;
    for mut span in content {
        // Clip anything past the end: offsets inside nested blocks (e.g.
        // code in a list item) can run past the source.
        span.range.end = span.range.end.min(len);
        // Skip anything overlapping what's already covered.
        if span.range.start < position || span.range.is_empty() {
            continue;
        }
        if span.range.start > position {
            spans.push(SourceSpan {
                range: position..span.range.start,
                style: SpanStyle::Syntax,
            });
        }
        position = span.range.end;
        spans.push(span);
    }
    if position < len {
        spans.push(SourceSpan {
            range: position..len,
            style: SpanStyle::Syntax,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::Blocks;

    /// Each span's source text, with `~` marking syntax.
    fn describe(text: &str) -> (RevealFont, Vec<String>) {
        let blocks = Blocks::parse(text);
        let block = &blocks.as_slice()[0];
        let source = &text[block.range.clone()];
        let styled = styled_source(source, block.kind, "");
        let spans = styled
            .spans
            .iter()
            .map(|span| {
                let text = &source[span.range.clone()];
                match span.style {
                    SpanStyle::Syntax => format!("~{text}"),
                    SpanStyle::Content(style) if style.strong => format!("B:{text}"),
                    SpanStyle::Content(style) if style.code => format!("C:{text}"),
                    SpanStyle::Content(style) if style.link => format!("L:{text}"),
                    SpanStyle::Content(_) => text.to_owned(),
                }
            })
            .collect();
        (styled.font, spans)
    }

    #[test]
    fn heading_marker_is_syntax() {
        assert_eq!(
            describe("## Title"),
            (
                RevealFont::Prose { heading: Some(2) },
                vec!["~## ".into(), "Title".into()]
            )
        );
    }

    #[test]
    fn inline_markers_are_syntax() {
        let (font, spans) = describe("Some **bold** and [a link](http://x).");
        assert_eq!(font, RevealFont::Prose { heading: None });
        assert_eq!(
            spans,
            [
                "Some ",
                "~**",
                "B:bold",
                "~**",
                " and ",
                "~[",
                "L:a link",
                "~](http://x)",
                "."
            ]
        );
    }

    #[test]
    fn list_markers_are_syntax() {
        let (_, spans) = describe("- one\n- two");
        assert_eq!(spans, ["~- ", "one", "~\n- ", "two"]);
    }

    #[test]
    fn blockquote_markers_are_syntax() {
        let (_, spans) = describe("> quoted\n> text");
        // The soft line break is content (it renders as a space) and merges
        // with the text before it.
        assert_eq!(spans, ["~> ", "quoted\n", "~> ", "text"]);
    }

    #[test]
    fn thematic_break_is_all_syntax() {
        assert_eq!(
            describe("---"),
            (RevealFont::Prose { heading: None }, vec!["~---".into()])
        );
    }

    #[test]
    fn code_tables_and_html_are_monospace() {
        for text in ["```\ncode\n```", "| a |\n|---|\n| 1 |", "<div>\nhi\n</div>"] {
            let (font, spans) = describe(text);
            assert_eq!(font, RevealFont::Monospace, "{text}");
            assert_eq!(spans, [text], "{text}");
        }
    }

    #[test]
    fn spans_cover_the_whole_source() {
        let text = "> - **a** `b`\n>   - [c][d]\n\n[d]: /x";
        let blocks = Blocks::parse(text);
        let block = &blocks.as_slice()[0];
        let source = &text[block.range.clone()];
        let styled = styled_source(source, block.kind, "[d]: /x");
        let mut position = 0;
        for span in &styled.spans {
            assert_eq!(span.range.start, position, "{styled:?}");
            position = span.range.end;
        }
        assert_eq!(position, source.len());
    }
}
