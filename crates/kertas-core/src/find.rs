//! Finding text in a document (#122). Matches are found in the text as it's
//! shown, so `**bold**` matches "bold", and are mapped back to where they
//! are in the source.

use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::html_tags;
use crate::inline::{InlineText, Segment};
use crate::model::Model;
use crate::rendered::RenderedBlock;

/// How a query matches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FindOptions {
    /// Upper and lower case differ. Off, "kertas" finds "Kertas".
    pub match_case: bool,
    /// Only whole words match: "cat" doesn't find "category".
    pub whole_word: bool,
}

/// What to look for, ready to search with.
#[derive(Debug, Clone)]
pub struct Query {
    regex: Regex,
    whole_word: bool,
}

impl Query {
    /// A query for `text` with `options`. `None` if `text` is empty, which
    /// finds nothing.
    pub fn new(text: &str, options: FindOptions) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let regex = RegexBuilder::new(&regex::escape(text))
            .case_insensitive(!options.match_case)
            .build()
            .ok()?;
        Some(Self {
            regex,
            whole_word: options.whole_word,
        })
    }

    /// Where the query matches in `text`, as byte ranges.
    fn matches_in<'a>(&'a self, text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.regex
            .find_iter(text)
            .map(|found| found.range())
            .filter(move |range| !range.is_empty())
            .filter(move |range| !self.whole_word || whole_word(text, range))
    }
}

/// Text the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// From where its first character is in the document's source to where
    /// its last one is, Markdown syntax between them included.
    pub range: Range<usize>,
    /// The source of its text, in order, without the syntax between: one
    /// piece, unless the match crosses formatting, as "bold" does in
    /// `bo**ld**`.
    pub pieces: Vec<Range<usize>>,
}

/// Every match for `query` in the document, in order. Each is within one
/// run of text, such as a paragraph, a heading or a table cell. Images'
/// alt text isn't searched, nor the numbers of footnote references.
pub fn find(model: &Model, query: &Query) -> Vec<Match> {
    let mut matches = Vec::new();
    for item in &model.items {
        let start = item.block.start;
        match item.rendered.as_ref() {
            // Shown as source, in full.
            RenderedBlock::Html(source) | RenderedBlock::Raw(source) => {
                if matches!(item.rendered.as_ref(), RenderedBlock::Html(_))
                    && html_tags::only_targets(source)
                {
                    continue;
                }
                matches.extend(query.matches_in(source).filter_map(|found| {
                    let range = start + found.start..start + found.end;
                    // It's drawn as its source, which it nearly always is.
                    (model.text.get(range.clone()) == Some(&source[found])).then(|| Match {
                        range: range.clone(),
                        pieces: vec![range],
                    })
                }));
            }
            block => block.each_text(&mut |text| {
                matches.extend(
                    query
                        .matches_in(&text.text)
                        .filter_map(|range| text_match(text, range, start)),
                )
            }),
        }
    }
    matches.sort_by_key(|found| (found.range.start, found.range.end));
    matches
}

/// The match at `display` in `text`, a run of text in the block starting
/// at `block_start`. `None` if it isn't text from the source, such as a
/// footnote reference's number, or it's in an image's alt text.
fn text_match(text: &InlineText, display: Range<usize>, block_start: usize) -> Option<Match> {
    let in_image = text
        .images
        .iter()
        .any(|image| image.display.start < display.end && display.start < image.display.end);
    let in_reference = text.links.iter().any(|link| {
        link.destination.starts_with("#fn-")
            && link.display.start < display.end
            && display.start < link.display.end
    });
    if in_image || in_reference {
        return None;
    }
    let mut pieces: Vec<Range<usize>> = Vec::new();
    for segment in &text.segments {
        let Some(source) = source_piece(segment, &display) else {
            continue;
        };
        let source = block_start + source.start..block_start + source.end;
        match pieces.last_mut() {
            Some(last) if last.end == source.start => last.end = source.end,
            _ => pieces.push(source),
        }
    }
    let range = pieces.first()?.start..pieces.last()?.end;
    Some(Match { range, pieces })
}

/// The source of the part of `display` in `segment`: exact if the segment's
/// text matches its source byte for byte, otherwise the whole segment's
/// source (an escape such as `\*`, say). `None` if they don't overlap, or
/// that part has no source.
fn source_piece(segment: &Segment, display: &Range<usize>) -> Option<Range<usize>> {
    let start = display.start.max(segment.display.start);
    let end = display.end.min(segment.display.end);
    if start >= end || segment.source.is_empty() {
        return None;
    }
    if segment.display.len() != segment.source.len() {
        return Some(segment.source.clone());
    }
    let at = |offset: usize| segment.source.start + offset - segment.display.start;
    Some(at(start)..at(end))
}

/// Whether `range` of `text` is whole words: no letter, digit or `_` runs
/// on from either end of it. An end that isn't a word character itself,
/// as in "-x", can be next to anything.
fn whole_word(text: &str, range: &Range<usize>) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let first = text[range.clone()].chars().next();
    let last = text[range.clone()].chars().next_back();
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    let joins = |edge: Option<char>, neighbor: Option<char>| {
        edge.is_some_and(word) && neighbor.is_some_and(word)
    };
    !joins(first, before) && !joins(last, after)
}

/// The match to go to from `offset`: the first that starts at or after it,
/// wrapping around to the first in the document. `None` if there are no
/// matches.
pub fn first_from(matches: &[Match], offset: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let after = matches.partition_point(|found| found.range.start < offset);
    Some(if after == matches.len() { 0 } else { after })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(text: &str, query: &str, options: FindOptions) -> Vec<Range<usize>> {
        let model = Model::new(text);
        let query = Query::new(query, options).unwrap();
        find(&model, &query)
            .into_iter()
            .map(|found| found.range)
            .collect()
    }

    fn found<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
        ranges(text, query, FindOptions::default())
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn matches_the_text_as_shown() {
        let text = "Some **bold** text, and *more* bold.\n";
        assert_eq!(found(text, "bold"), ["bold", "bold"]);
        assert_eq!(ranges(text, "bold", FindOptions::default())[0], 7..11);
        // The syntax isn't searched.
        assert!(found(text, "**").is_empty());
        assert!(found(text, "*more").is_empty());
    }

    #[test]
    fn ignores_case_unless_asked() {
        let text = "Kertas and kertas.\n";
        assert_eq!(found(text, "kertas").len(), 2);
        let options = FindOptions {
            match_case: true,
            ..FindOptions::default()
        };
        assert_eq!(ranges(text, "kertas", options), [11..17]);
    }

    #[test]
    fn whole_words_only() {
        let text = "A cat, a category, a bobcat and the cat's toy.\n";
        let options = FindOptions {
            whole_word: true,
            ..FindOptions::default()
        };
        assert_eq!(ranges(text, "cat", options).len(), 2);
        assert_eq!(found(text, "cat").len(), 4);
        assert!(whole_word("a -x b", &(2..4)));
        assert!(whole_word("a-x b", &(1..3)));
        assert!(!whole_word("ab", &(0..1)));
    }

    #[test]
    fn a_match_across_formatting_leaves_out_the_syntax() {
        let text = "bo**ld**\n";
        let model = Model::new(text);
        let query = Query::new("bold", FindOptions::default()).unwrap();
        let matches = find(&model, &query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].range, 0..6);
        assert_eq!(matches[0].pieces, [0..2, 4..6]);
    }

    #[test]
    fn searches_every_kind_of_text() {
        let text = "# A fox\n\n- a fox\n\n> a fox\n\n| fox |\n|-----|\n| fox |\n\n```\nfox\n```\n\n<div>fox</div>\n";
        assert_eq!(found(text, "fox").len(), 7);
    }

    #[test]
    fn skips_image_alt_text_and_link_destinations() {
        let text = "![a fox](fox.png) and [a fox](fox.md)\n";
        assert_eq!(ranges(text, "fox", FindOptions::default()).len(), 1);
    }

    #[test]
    fn goes_from_an_offset_wrapping_around() {
        let text = "fox fox fox\n";
        let model = Model::new(text);
        let matches = find(&model, &Query::new("fox", FindOptions::default()).unwrap());
        assert_eq!(first_from(&matches, 0), Some(0));
        assert_eq!(first_from(&matches, 1), Some(1));
        assert_eq!(first_from(&matches, 9), Some(0));
        assert_eq!(first_from(&[], 0), None);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(Query::new("", FindOptions::default()).is_none());
    }
}
