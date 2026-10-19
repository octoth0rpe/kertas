//! A document's link reference definitions, like `[label]: https://…`, so
//! reference links (`[text][label]`) resolve when a block is parsed on its
//! own, and its footnotes' numbers (#90).
//!
//! The table is built once per parse of the whole document (see
//! [`crate::blocks::Blocks`]). Each block's parser consults it only for
//! references the block itself doesn't define, through pulldown-cmark's
//! broken-link callback, so rendering costs the same however many
//! definitions there are.
//!
//! Footnote references have no such callback: pulldown-cmark only treats
//! `[^label]` as a reference if the text it parses defines `label`. So a
//! block that refers to notes defined elsewhere is parsed with a stand-in
//! definition for each appended after it, and anything the parser makes of
//! those is dropped.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{Event, Parser, RefDefs};

use crate::blocks::{Syntax, normalize_label as normalize, parser_options_for};

/// Link definitions by label, and footnote numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Definitions {
    /// Destination and title, keyed by the normalized label.
    links: HashMap<String, (String, String)>,
    /// Each referenced footnote's number, keyed by its normalized label:
    /// 1 for the first one referred to, and so on.
    footnotes: HashMap<String, usize>,
    /// The syntax the document is read with, which every block is parsed
    /// with too.
    syntax: Syntax,
}

impl Definitions {
    /// The definitions a parser found in the whole document, and the
    /// labels of its footnote references in order.
    pub(crate) fn from_parser<'a>(
        definitions: &RefDefs,
        footnote_references: impl IntoIterator<Item = &'a str>,
        syntax: Syntax,
    ) -> Self {
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
        let mut footnotes = HashMap::new();
        for label in footnote_references {
            let next = footnotes.len() + 1;
            footnotes.entry(label.to_owned()).or_insert(next);
        }
        Self {
            links,
            footnotes,
            syntax,
        }
    }

    /// The syntax the document is read with.
    pub fn syntax(&self) -> Syntax {
        self.syntax
    }

    /// The definitions in `text`, which is parsed as a whole document.
    pub fn from_text(text: &str) -> Self {
        Self::from_text_with(text, Syntax::default())
    }

    /// The definitions in `text`, read with `syntax`.
    pub fn from_text_with(text: &str, syntax: Syntax) -> Self {
        crate::blocks::Blocks::parse_with(text, syntax)
            .definitions()
            .clone()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty() && self.footnotes.is_empty()
    }

    /// The number of the footnote `label` (as written), if it's referred
    /// to.
    pub fn footnote_number(&self, label: &str) -> Option<usize> {
        self.footnotes.get(&normalize(label)).copied()
    }

    /// `source` (one block), followed by a stand-in definition for each
    /// numbered footnote it may refer to, to parse with [`Self::parse`].
    pub fn prepare<'a>(&self, source: &'a str) -> Cow<'a, str> {
        if self.footnotes.is_empty() || !source.contains("[^") {
            return Cow::Borrowed(source);
        }
        let mut text = source.to_owned();
        let mut added = Vec::new();
        for (start, _) in source.match_indices("[^") {
            let rest = &source[start + 2..];
            let Some(end) = rest.find([']', '[']) else {
                continue;
            };
            let label = normalize(&rest[..end]);
            if self.footnotes.contains_key(&label) && !added.contains(&label) {
                text.push_str("\n\n[^");
                text.push_str(&label);
                text.push_str("]: .");
                added.push(label);
            }
        }
        Cow::Owned(text)
    }

    /// Parses `text`, made by [`Self::prepare`] from a block `len` bytes
    /// long, with Kertas's options, resolving reference links against
    /// these definitions. Events from the stand-in footnotes are dropped.
    pub fn parse<'a>(
        &'a self,
        text: &'a str,
        len: usize,
    ) -> impl Iterator<Item = (Event<'a>, Range<usize>)> + 'a {
        let resolve = move |link: pulldown_cmark::BrokenLink<'a>| {
            let (destination, title) = self.links.get(&normalize(&link.reference))?;
            Some((destination.clone().into(), title.clone().into()))
        };
        Parser::new_with_broken_link_callback(text, parser_options_for(self.syntax), Some(resolve))
            .into_offset_iter()
            .filter_map(move |(event, range)| {
                if range.end <= len || text.len() == len {
                    return Some((event, range));
                }
                if range.start > len {
                    return None;
                }
                match event {
                    // An unclosed code block runs on into the stand-ins:
                    // keep the part that's in the block.
                    Event::Text(content) => {
                        let content = content.get(..len - range.start)?.to_owned();
                        Some((Event::Text(content.into()), range.start..len))
                    }
                    _ => Some((event, range)),
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::{Event, Tag};

    use super::*;

    /// The destinations of the links in `source`.
    fn links(definitions: &Definitions, source: &str) -> Vec<String> {
        let text = definitions.prepare(source);
        definitions
            .parse(&text, source.len())
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

    /// The footnote numbers each reference in `source` shows.
    fn footnotes(definitions: &Definitions, source: &str) -> Vec<usize> {
        let text = definitions.prepare(source);
        definitions
            .parse(&text, source.len())
            .filter_map(|(event, _)| match event {
                Event::FootnoteReference(label) => definitions.footnote_number(&label),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn footnotes_are_numbered_by_first_reference() {
        let definitions =
            Definitions::from_text("B[^b], A[^a], B again[^B].\n\n[^a]: A.\n[^b]: B.\n[^c]: C.\n");
        assert_eq!(definitions.footnote_number("b"), Some(1));
        assert_eq!(definitions.footnote_number("A"), Some(2));
        assert_eq!(definitions.footnote_number("c"), None, "never referenced");
    }

    #[test]
    fn references_to_notes_defined_elsewhere_resolve() {
        let definitions = Definitions::from_text("x[^one]\n\n[^one]: One.\n");
        assert_eq!(footnotes(&definitions, "See[^one] and[^ONE]."), [1, 1]);
        assert!(footnotes(&definitions, "Not a note[^two].").is_empty());
        assert!(footnotes(&Definitions::default(), "x[^one]").is_empty());
    }

    #[test]
    fn stand_in_notes_add_nothing_to_the_block() {
        let definitions = Definitions::from_text("x[^one]\n\n[^one]: One.\n");
        // An unclosed code block would otherwise run on into them.
        let source = "```\nx[^one]";
        let text = definitions.prepare(source);
        assert_ne!(text, source);
        let code: String = definitions
            .parse(&text, source.len())
            .filter_map(|(event, _)| match event {
                Event::Text(text) => Some(text.into_string()),
                _ => None,
            })
            .collect();
        assert_eq!(code, "x[^one]");
    }
}
