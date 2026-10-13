//! A document's link reference definitions, like `[label]: https://…`, so
//! reference links (`[text][label]`) resolve when a block is parsed on its
//! own.
//!
//! The table is built once per parse of the whole document (see
//! [`crate::blocks::Blocks`]). Each block's parser consults it only for
//! references the block itself doesn't define, through pulldown-cmark's
//! broken-link callback, so rendering costs the same however many
//! definitions there are.

use std::collections::HashMap;

use pulldown_cmark::{BrokenLink, CowStr, OffsetIter, Parser, RefDefs};

use crate::blocks::parser_options;

/// Link definitions by label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Definitions {
    /// Destination and title, keyed by the normalized label.
    links: HashMap<String, (String, String)>,
}

impl Definitions {
    /// The definitions a parser found in the whole document.
    pub(crate) fn from_parser(definitions: &RefDefs) -> Self {
        let links = definitions
            .iter()
            .map(|(label, definition)| {
                (
                    normalize(label),
                    (
                        definition.dest.to_string(),
                        definition.title.as_deref().unwrap_or_default().to_owned(),
                    ),
                )
            })
            .collect();
        Self { links }
    }

    /// The definitions in `text`, which is parsed as a whole document.
    pub fn from_text(text: &str) -> Self {
        let parser = Parser::new_ext(text, parser_options());
        Self::from_parser(parser.reference_definitions())
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Parses `source` (one block) with Kertas's options, resolving reference
    /// links against these definitions.
    pub fn parse<'a>(
        &'a self,
        source: &'a str,
    ) -> OffsetIter<'a, impl FnMut(BrokenLink<'a>) -> Option<(CowStr<'a>, CowStr<'a>)> + 'a> {
        let resolve = move |link: BrokenLink<'a>| {
            let (destination, title) = self.links.get(&normalize(&link.reference))?;
            Some((destination.clone().into(), title.clone().into()))
        };
        Parser::new_with_broken_link_callback(source, parser_options(), Some(resolve))
            .into_offset_iter()
    }
}

/// A label as CommonMark matches it: case-insensitive, with runs of
/// whitespace treated as one space.
fn normalize(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::{Event, Tag};

    use super::*;

    /// The destinations of the links in `source`.
    fn links(definitions: &Definitions, source: &str) -> Vec<String> {
        definitions
            .parse(source)
            .filter_map(|(event, _)| match event {
                Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn resolves_every_reference_form() {
        let definitions = Definitions::from_text("[docs]: /docs \"The docs\"\n");
        assert_eq!(
            links(&definitions, "[full][docs], [docs][] and [docs]"),
            ["/docs", "/docs", "/docs"]
        );
    }

    #[test]
    fn labels_match_ignoring_case_and_spacing() {
        let definitions = Definitions::from_text("[The  Docs]: /docs\n");
        assert_eq!(links(&definitions, "[see][the docs]"), ["/docs"]);
        assert_eq!(links(&definitions, "[THE\nDOCS]"), ["/docs"]);
    }

    #[test]
    fn undefined_references_stay_text() {
        let definitions = Definitions::from_text("[docs]: /docs\n");
        assert!(links(&definitions, "[nothing][here]").is_empty());
        assert!(Definitions::default().is_empty());
    }

    #[test]
    fn definitions_nested_in_blocks_count() {
        let definitions = Definitions::from_text("- item\n\n  [nested]: /nested\n");
        assert_eq!(links(&definitions, "[nested]"), ["/nested"]);
    }

    #[test]
    fn the_first_definition_wins() {
        let definitions = Definitions::from_text("[a]: /first\n[a]: /second\n");
        assert_eq!(links(&definitions, "[a]"), ["/first"]);
    }
}
