//! Counting the words in a document, as a writer would: the words of its
//! prose, not its Markdown syntax.

use unicode_segmentation::UnicodeSegmentation;

use crate::inline::InlineText;
use crate::model::Model;
use crate::rendered::RenderedBlock;

/// How many words `text` has, read as Markdown. See [`count_in`].
pub fn count(text: &str) -> usize {
    count_in(&Model::new(text))
}

/// How many words the document in `model` has. Only the text as it reads
/// is counted: headings, paragraphs, lists, quotes and callouts, tables
/// and footnotes. Markdown syntax, link destinations, code blocks, HTML and
/// front matter aren't. Words are split as Unicode splits them, so
/// "don't" is one word and "half-term" two.
pub fn count_in(model: &Model) -> usize {
    model
        .items
        .iter()
        .map(|item| block_words(&item.rendered))
        .sum()
}

fn block_words(block: &RenderedBlock) -> usize {
    match block {
        RenderedBlock::Paragraph(text) | RenderedBlock::Heading { text, .. } => words(text),
        RenderedBlock::List { items, .. } => items
            .iter()
            .flat_map(|item| &item.children)
            .map(block_words)
            .sum(),
        RenderedBlock::BlockQuote(children)
        | RenderedBlock::Alert { children, .. }
        | RenderedBlock::FootnoteDefinition { children, .. } => {
            children.iter().map(block_words).sum()
        }
        RenderedBlock::Table(table) => table
            .header
            .iter()
            .chain(table.rows.iter().flatten())
            .map(words)
            .sum(),
        RenderedBlock::ThematicBreak
        | RenderedBlock::CodeBlock { .. }
        | RenderedBlock::Html(_)
        | RenderedBlock::Raw(_) => 0,
    }
}

/// The words in `text`, leaving out footnote references' numbers and
/// notes' back-links, which are links within the document.
fn words(text: &InlineText) -> usize {
    let mut prose = text.text.clone();
    for link in &text.links {
        if link.destination.starts_with("#fn-") || link.destination.starts_with("#fnref-") {
            let blank = " ".repeat(link.display.len());
            prose.replace_range(link.display.clone(), &blank);
        }
    }
    prose.unicode_words().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_prose() {
        assert_eq!(count(""), 0);
        assert_eq!(count("It was a dark and stormy night."), 7);
        assert_eq!(count("Don't stop, half-term's near."), 5);
    }

    #[test]
    fn ignores_markdown_syntax() {
        assert_eq!(
            count("# The *Storm*\n\n**Rain** fell [hard](https://example.com/a/b)."),
            5
        );
        assert_eq!(count("- one\n- two\n  > three\n\n1. four\n"), 4);
        assert_eq!(count("> [!NOTE]\n> Callouts count.\n"), 2);
        assert_eq!(count("| A | B c |\n|---|---|\n| d | e |\n"), 5);
        assert_eq!(count("Text.[^1] More.[^1]\n\n[^1]: A note.\n"), 4);
    }

    #[test]
    fn skips_code_html_and_front_matter() {
        let text = "---\ntitle: Many words here\n---\n\nOne.\n\n```\nlet x = 1;\n```\n\n<div>\nhidden words\n</div>\n\n---\n\nTwo `inline code`.\n";
        assert_eq!(count(text), 4);
    }
}
