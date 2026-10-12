//! What each top-level block looks like when rendered.

use crate::blocks::{BlockKind, Blocks};
use crate::inline::{InlineText, render_inline};

/// A block prepared for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedBlock {
    Paragraph(InlineText),
    Heading {
        level: u8,
        text: InlineText,
    },
    ThematicBreak,
    /// A block kami doesn't render yet, shown as its source text.
    Raw(String),
}

/// Prepares every block in `text` for display.
pub fn render_blocks(text: &str, blocks: &Blocks) -> Vec<RenderedBlock> {
    let definitions = blocks
        .as_slice()
        .iter()
        .filter(|block| block.kind == BlockKind::LinkReferenceDefinition)
        .map(|block| &text[block.range.clone()])
        .collect::<Vec<_>>()
        .join("\n");

    blocks
        .as_slice()
        .iter()
        .map(|block| {
            let source = &text[block.range.clone()];
            match block.kind {
                BlockKind::Paragraph => {
                    RenderedBlock::Paragraph(render_inline(source, &definitions))
                }
                BlockKind::Heading { level } => RenderedBlock::Heading {
                    level,
                    text: render_inline(source, &definitions),
                },
                BlockKind::ThematicBreak => RenderedBlock::ThematicBreak,
                _ => RenderedBlock::Raw(source.to_owned()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(text: &str) -> Vec<RenderedBlock> {
        render_blocks(text, &Blocks::parse(text))
    }

    fn display_text(block: &RenderedBlock) -> &str {
        match block {
            RenderedBlock::Paragraph(text) | RenderedBlock::Heading { text, .. } => &text.text,
            RenderedBlock::Raw(source) => source,
            RenderedBlock::ThematicBreak => "---",
        }
    }

    #[test]
    fn text_blocks_are_rendered_and_others_raw() {
        let blocks = render("# Title\n\nSome *text*.\n\n***\n\n- list\n\n> quote\n");
        let texts: Vec<_> = blocks.iter().map(display_text).collect();
        assert_eq!(texts, ["Title", "Some text.", "---", "- list", "> quote"]);
        assert!(matches!(blocks[0], RenderedBlock::Heading { level: 1, .. }));
        assert!(matches!(blocks[3], RenderedBlock::Raw(_)));
    }

    #[test]
    fn reference_links_use_definitions_anywhere_in_document() {
        let blocks = render("See [kami][k].\n\n[k]: https://example.com\n");
        assert_eq!(display_text(&blocks[0]), "See kami.");
        assert_eq!(
            blocks[1],
            RenderedBlock::Raw("[k]: https://example.com".into())
        );
    }

    #[test]
    fn large_document_renders_quickly() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/spikes/fixtures/10k.md"
        ))
        .unwrap();
        let start = std::time::Instant::now();
        let blocks = Blocks::parse(&text);
        let rendered = render_blocks(&text, &blocks);
        let elapsed = start.elapsed();
        assert_eq!(rendered.len(), blocks.len());
        // Generous bound for unoptimized test builds; N1 is 500 ms end to end.
        assert!(elapsed.as_millis() < 250, "took {elapsed:?}");
        eprintln!("parsed and rendered {} blocks in {elapsed:?}", blocks.len());
    }
}
