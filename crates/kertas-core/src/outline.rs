//! A document's outline: its headings, in order, for a host app to show
//! as a table of contents.

use std::ops::Range;

use crate::anchors::{self, HeadingAnchors, Target};
use crate::model::Model;
use crate::rendered::RenderedBlock;

/// A heading in a document's outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `#`, up to 6 for `######`.
    pub level: u8,
    /// Its text, without Markdown syntax.
    pub text: String,
    /// The name a `#name` link to it uses, as [`anchors::anchors`] gives
    /// it.
    pub anchor: String,
    /// Where its block is in the document's text.
    pub range: Range<usize>,
}

/// The headings in `model`, in document order. Only headings at the top
/// level are listed, not those in quotes or lists, but their anchors are
/// numbered counting those too, as links to them are.
pub fn headings(model: &Model) -> Vec<Heading> {
    let mut anchors = HeadingAnchors::default();
    let mut headings = Vec::new();
    for item in &model.items {
        let mut targets = Vec::new();
        anchors::collect(&item.rendered, &mut targets);
        // A heading at the top level is its item's first target.
        let mut first = true;
        for target in targets {
            let Target::Heading(text) = target else {
                continue;
            };
            let anchor = anchors.next(&text);
            if let (true, RenderedBlock::Heading { level, .. }) = (first, &*item.rendered) {
                headings.push(Heading {
                    level: *level,
                    text,
                    anchor,
                    range: item.block.clone(),
                });
            }
            first = false;
        }
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_top_level_headings() {
        let text = "# The *Storm*\n\nRain.\n\n> # Quoted\n\n## Notes\n\nText\n\n## Notes\n";
        let model = Model::new(text);
        let headings = headings(&model);
        let summary: Vec<_> = headings
            .iter()
            .map(|heading| {
                (
                    heading.level,
                    heading.text.as_str(),
                    heading.anchor.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (1, "The Storm", "the-storm"),
                (2, "Notes", "notes"),
                (2, "Notes", "notes-1")
            ]
        );
        assert_eq!(&text[headings[0].range.clone()], "# The *Storm*");
        assert_eq!(&text[headings[2].range.clone()], "## Notes");
    }

    #[test]
    fn anchors_match_links() {
        let model = Model::new("> ## Setup\n\n## Setup\n\nSetup\n=====\n");
        let outline: Vec<_> = headings(&model)
            .into_iter()
            .map(|heading| heading.anchor)
            .collect();
        assert_eq!(outline, ["setup-1", "setup-2"]);
        for anchor in outline {
            assert!(anchors::find(&model, &anchor).is_some());
        }
    }

    #[test]
    fn no_headings() {
        assert_eq!(headings(&Model::new("")), []);
        assert_eq!(headings(&Model::new("Just text.\n")), []);
    }
}
