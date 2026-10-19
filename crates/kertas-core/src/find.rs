//! Finding text in a document (#122), optionally with a regular expression
//! (#124). Matches are found in the text as it's shown, so `**bold**`
//! matches "bold", and are mapped back to where they are in the source.

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
    /// The query is a regular expression, in Rust's `regex` syntax, and a
    /// replacement can use its groups (`$1`, `${name}`).
    pub regex: bool,
}

/// Which options a find bar offers (#125). A host can leave some out; an
/// option that isn't offered stays off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfferedOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl Default for OfferedOptions {
    /// Every option.
    fn default() -> Self {
        Self {
            match_case: true,
            whole_word: true,
            regex: true,
        }
    }
}

impl FindOptions {
    /// These options, with those that aren't `offered` turned off.
    pub fn offered(self, offered: OfferedOptions) -> Self {
        Self {
            match_case: self.match_case && offered.match_case,
            whole_word: self.whole_word && offered.whole_word,
            regex: self.regex && offered.regex,
        }
    }
}

/// The most memory a regular expression may compile to. Matching takes
/// time in proportion to the text whatever the pattern, so none hangs, but
/// a huge one (`a{1000}{1000}`) is turned down rather than built.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// What to look for, ready to search with.
#[derive(Debug, Clone)]
pub struct Query {
    regex: Regex,
    whole_word: bool,
    /// Whether a replacement's `$` refer to the regular expression's
    /// groups. Otherwise it's taken as it is.
    expands: bool,
}

impl Query {
    /// A query for `text` with `options`. `Ok(None)` if `text` is empty,
    /// which finds nothing, and an error saying what's wrong if it's a
    /// regular expression that isn't valid.
    pub fn new(text: &str, options: FindOptions) -> Result<Option<Self>, String> {
        if text.is_empty() {
            return Ok(None);
        }
        let pattern = if options.regex {
            text.to_owned()
        } else {
            regex::escape(text)
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!options.match_case)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map_err(|err| error_message(&err))?;
        Ok(Some(Self {
            regex,
            whole_word: options.whole_word,
            expands: options.regex,
        }))
    }

    /// Where the query matches in `text`, as byte ranges, with what
    /// `template` replaces each with, if there is one. Empty matches,
    /// which a pattern like `a*` finds everywhere, are left out.
    fn matches_in(&self, text: &str, template: Option<&str>) -> Vec<(Range<usize>, String)> {
        let keep = |range: &Range<usize>| {
            !range.is_empty() && (!self.whole_word || whole_word(text, range))
        };
        match template {
            Some(template) if self.expands => self
                .regex
                .captures_iter(text)
                .filter_map(|captures| {
                    let range = captures.get_match().range();
                    let mut replacement = String::new();
                    captures.expand(template, &mut replacement);
                    keep(&range).then_some((range, replacement))
                })
                .collect(),
            _ => self
                .regex
                .find_iter(text)
                .map(|found| found.range())
                .filter(keep)
                .map(|range| (range, template.unwrap_or_default().to_owned()))
                .collect(),
        }
    }
}

/// What's wrong with a regular expression, in a line: the regex crate's
/// own message, without the pattern and the caret it draws under it.
fn error_message(err: &regex::Error) -> String {
    match err {
        regex::Error::Syntax(message) => message
            .lines()
            .find_map(|line| line.strip_prefix("error: "))
            .unwrap_or("not a valid regular expression")
            .to_owned(),
        regex::Error::CompiledTooBig(_) => "too large".to_owned(),
        _ => "not a valid regular expression".to_owned(),
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

impl Match {
    /// The edits that replace the match with `text`, leaving the rest of
    /// the source as it is: the first piece becomes `text`, and any others
    /// go, keeping the syntax between them.
    pub fn replacement(&self, text: &str) -> Vec<(Range<usize>, String)> {
        self.pieces
            .iter()
            .enumerate()
            .map(|(index, piece)| {
                let new = if index == 0 { text } else { "" };
                (piece.clone(), new.to_owned())
            })
            .collect()
    }
}

/// Every match for `query` in the document, in order. Each is within one
/// run of text, such as a paragraph, a heading or a table cell. Images'
/// alt text isn't searched, nor the numbers of footnote references.
pub fn find(model: &Model, query: &Query) -> Vec<Match> {
    search(model, query, None)
        .into_iter()
        .map(|(found, _)| found)
        .collect()
}

/// Every match for `query`, as [`find`] finds them, with the text
/// `template` replaces each with: `template` itself, or for a regular
/// expression, `template` with its `$1` and `${name}` filled in from the
/// match's groups.
pub fn replacements(model: &Model, query: &Query, template: &str) -> Vec<(Match, String)> {
    search(model, query, Some(template))
}

fn search(model: &Model, query: &Query, template: Option<&str>) -> Vec<(Match, String)> {
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
                let found = query.matches_in(source, template);
                matches.extend(found.into_iter().filter_map(|(found, replacement)| {
                    let range = start + found.start..start + found.end;
                    // It's drawn as its source, which it nearly always is.
                    let shown = model.text.get(range.clone()) == Some(&source[found]);
                    shown.then(|| {
                        let pieces = vec![range.clone()];
                        (Match { range, pieces }, replacement)
                    })
                }));
            }
            block => block.each_text(&mut |text| {
                let found = query.matches_in(&text.text, template);
                matches.extend(found.into_iter().filter_map(|(range, replacement)| {
                    Some((text_match(text, range, start)?, replacement))
                }))
            }),
        }
    }
    matches.sort_by_key(|(found, _)| (found.range.start, found.range.end));
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
        let query = Query::new(query, options).unwrap().unwrap();
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
        let query = Query::new("bold", FindOptions::default()).unwrap().unwrap();
        let matches = find(&model, &query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].range, 0..6);
        assert_eq!(matches[0].pieces, [0..2, 4..6]);
    }

    #[test]
    fn replacing_changes_only_the_matched_text() {
        let text = "Some **bold** text and bo**ld**.\n";
        let model = Model::new(text);
        let matches = find(
            &model,
            &Query::new("bold", FindOptions::default()).unwrap().unwrap(),
        );
        let mut replaced = text.to_owned();
        for found in matches.iter().rev() {
            for (range, new) in found.replacement("strong").into_iter().rev() {
                replaced.replace_range(range, &new);
            }
        }
        assert_eq!(replaced, "Some **strong** text and strong****.\n");
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
        let matches = find(
            &model,
            &Query::new("fox", FindOptions::default()).unwrap().unwrap(),
        );
        assert_eq!(first_from(&matches, 0), Some(0));
        assert_eq!(first_from(&matches, 1), Some(1));
        assert_eq!(first_from(&matches, 9), Some(0));
        assert_eq!(first_from(&[], 0), None);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(Query::new("", FindOptions::default()).unwrap().is_none());
    }

    fn regex() -> FindOptions {
        FindOptions {
            regex: true,
            ..FindOptions::default()
        }
    }

    #[test]
    fn a_plain_query_is_taken_literally() {
        assert_eq!(found("Cost: $5 (or a.b)\n", "$5 (or a."), ["$5 (or a."]);
        assert!(found("Cost: $5 (or a.b)\n", "a.c").is_empty());
    }

    #[test]
    fn regular_expressions_match_the_text_as_shown() {
        let text = "Ann met **Bob**, then Cy.\n";
        let case = FindOptions {
            match_case: true,
            ..regex()
        };
        let names: Vec<&str> = ranges(text, r"\b[A-Z][a-z]+\b", case)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(names, ["Ann", "Bob", "Cy"]);
        // `^` is the start of a run of text, not of its source.
        assert_eq!(ranges("**Bold** start\n", "^bold", regex()), [2..6]);
    }

    #[test]
    fn replacements_fill_in_groups() {
        let model = Model::new("Smith, Ann and Jones, Bob.\n");
        let query = Query::new(r"(\w+), (?<first>\w+)", regex())
            .unwrap()
            .unwrap();
        let replaced: Vec<String> = replacements(&model, &query, "${first} $1")
            .into_iter()
            .map(|(_, replacement)| replacement)
            .collect();
        assert_eq!(replaced, ["Ann Smith", "Bob Jones"]);
        // Without Regex, `$` is just a dollar sign.
        let plain = Query::new("Ann", FindOptions::default()).unwrap().unwrap();
        let replaced = replacements(&model, &plain, "$1");
        assert_eq!(replaced[0].1, "$1");
    }

    #[test]
    fn an_invalid_pattern_says_why() {
        assert_eq!(
            Query::new("(unclosed", regex()).unwrap_err(),
            "unclosed group"
        );
        assert_eq!(
            Query::new("a{1000}{1000}", regex()).unwrap_err(),
            "too large"
        );
        // The same text is fine as a plain query.
        assert!(Query::new("(unclosed", FindOptions::default()).is_ok());
    }

    #[test]
    fn empty_matches_are_left_out() {
        assert!(ranges("abc\n", "x*", regex()).is_empty());
        assert_eq!(ranges("abc\n", "b*", regex()), [1..2]);
    }
}
