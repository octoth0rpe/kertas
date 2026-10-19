//! The words to check for spelling (#139): a document's prose, as it's
//! shown, each word with where it is in the source.
//!
//! Only prose is checked. Code, code blocks, HTML tags, front matter, link
//! reference definitions, image alt text and footnote numbers are left
//! out, and so are web addresses and email addresses, written out or as a
//! link's text. A link's text is prose, but where it leads isn't.
//!
//! A word is a run of letters, which may hold apostrophes (`don't`,
//! `o’clock`) and hyphens (`well-known`) between them. Words with digits
//! or underscores (`mp3`, `snake_case`), and words in capitals, which are
//! usually acronyms (`NASA`), aren't checked. A possessive is checked
//! without its `'s` or its trailing apostrophe: `Mikey's` as `Mikey`, and
//! `Holmes'` as `Holmes`.

use std::ops::Range;

use crate::find::text_match;
use crate::inline::InlineText;
use crate::model::Model;
use crate::rendered::RenderedBlock;

/// A word to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// Where it is in the document's source, Markdown syntax inside it
    /// included, as in `bo**ld**`.
    pub range: Range<usize>,
    /// The word to look up: as shown, without a possessive ending.
    pub word: String,
}

/// The words to check in `model`, in order.
pub fn words(model: &Model) -> Vec<Word> {
    let mut words = Vec::new();
    for item in &model.items {
        words.extend(block_words(&item.rendered, item.block.start));
    }
    words
}

/// The words to check in `block`, which starts at `block_start` in the
/// source, in order.
pub fn block_words(block: &RenderedBlock, block_start: usize) -> Vec<Word> {
    let mut words = Vec::new();
    prose(block, &mut |text| {
        for display in text_words(text) {
            let word = lookup_form(&text.text[display.clone()]);
            if let Some(found) = text_match(text, display, block_start) {
                words.push(Word {
                    range: found.range,
                    word,
                });
            }
        }
    });
    words
}

/// Calls `f` with each run of prose in `block`: every text but code
/// blocks'.
fn prose(block: &RenderedBlock, f: &mut dyn FnMut(&InlineText)) {
    match block {
        RenderedBlock::CodeBlock { .. } => {}
        RenderedBlock::List { items, .. } => {
            for child in items.iter().flat_map(|item| &item.children) {
                prose(child, f);
            }
        }
        RenderedBlock::BlockQuote(children)
        | RenderedBlock::Alert { children, .. }
        | RenderedBlock::FootnoteDefinition { children, .. } => {
            for child in children {
                prose(child, f);
            }
        }
        block => block.each_text(f),
    }
}

/// Whether `c` can be inside a word, between letters.
fn joins(c: char) -> bool {
    matches!(c, '\'' | '’' | '-' | '‐')
}

/// The ranges of the words to check in `text`'s display text.
fn text_words(text: &InlineText) -> Vec<Range<usize>> {
    let shown = &text.text;
    // Footnote numbers and images aren't text, and end any word they
    // touch: `word[^1]` is shown as "word1".
    let breaks: Vec<Range<usize>> = text
        .links
        .iter()
        .filter(|link| link.destination.starts_with("#fn-"))
        .map(|link| link.display.clone())
        .chain(text.images.iter().map(|image| image.display.clone()))
        .collect();
    let is_break = |at: usize| breaks.iter().any(|range| range.contains(&at));
    // Code and web addresses are left out, wherever words are.
    let skipped: Vec<Range<usize>> = text
        .segments
        .iter()
        .filter(|segment| segment.style.code)
        .map(|segment| segment.display.clone())
        .chain(addresses(shown))
        .chain(tags(shown))
        .collect();
    let mut words = Vec::new();
    let mut start = None;
    let chars: Vec<(usize, char)> = shown.char_indices().collect();
    for (index, &(at, c)) in chars.iter().enumerate() {
        let in_word = (c.is_alphanumeric() || c == '_' || is_mark(c)) && !is_break(at);
        // An apostrophe or hyphen stays in a word only between letters.
        let between = joins(c)
            && start.is_some()
            && chars
                .get(index + 1)
                .is_some_and(|&(next_at, next)| next.is_alphanumeric() && !is_break(next_at));
        match (start, in_word || between) {
            (None, true) if !joins(c) => start = Some(at),
            (Some(from), false) => {
                words.push(from..at);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push(from..shown.len());
    }
    words
        .into_iter()
        .filter(|range| {
            let word = &shown[range.clone()];
            checked(word)
                && !skipped
                    .iter()
                    .any(|skip| skip.start < range.end && range.start < skip.end)
        })
        .collect()
}

/// Whether `c` is a combining mark, part of the letter before it, as in a
/// decomposed `é`.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{20D0}'..='\u{20FF}')
}

/// Whether `word` is checked: it has no digits or underscores, and isn't
/// in capitals.
fn checked(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let capitals = word.chars().filter(|c| c.is_uppercase()).count();
    !word.chars().any(|c| c.is_numeric() || c == '_') && !(letters > 1 && capitals == letters)
}

/// `word` as it's looked up: without a possessive `'s`, or the apostrophe
/// a trailing possessive ends with, which isn't part of a word's range.
fn lookup_form(word: &str) -> String {
    for ending in ["'s", "’s", "'S", "’S"] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            return stem.to_owned();
        }
    }
    word.to_owned()
}

/// The HTML tags shown in `text`, as Kertas shows inline HTML: `<b>`,
/// `</span>`, `<br/>`, comments.
fn tags(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut rest = 0;
    while let Some(open) = text[rest..].find('<').map(|at| rest + at) {
        let after = &text[open + 1..];
        let starts_tag = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
        match after.find(['>', '<']) {
            Some(end) if starts_tag && after.as_bytes()[end] == b'>' => {
                found.push(open..open + 1 + end + 1);
                rest = open + 1 + end + 1;
            }
            _ => rest = open + 1,
        }
    }
    found
}

/// The web and email addresses written out in `text`: `https://…`,
/// `www.…` and `name@host`, each up to the next space.
fn addresses(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        let lower = word.to_ascii_lowercase();
        let address = lower.contains("://")
            || lower.starts_with("www.")
            || lower.starts_with("mailto:")
            || (word.contains('@') && word.contains('.'));
        if address {
            found.push(at..at + word.len());
        }
        at += piece.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::Syntax;

    /// The words to check in `source`, each as its source text and the
    /// word looked up.
    fn found(source: &str) -> Vec<(&str, String)> {
        words(&Model::new_with(source, Syntax { wiki_links: true }))
            .into_iter()
            .map(|word| (&source[word.range], word.word))
            .collect()
    }

    /// Just the words looked up.
    fn looked_up(source: &str) -> Vec<String> {
        found(source).into_iter().map(|(_, word)| word).collect()
    }

    #[test]
    fn prose_words_with_their_places() {
        assert_eq!(
            found("# A *grey* day\n\nIt was **bo**ld.\n"),
            [
                ("A", "A".into()),
                ("grey", "grey".into()),
                ("day", "day".into()),
                ("It", "It".into()),
                ("was", "was".into()),
                ("bo**ld", "bold".into()),
            ]
        );
    }

    #[test]
    fn apostrophes_and_hyphens_inside_words() {
        assert_eq!(
            looked_up("Don't o’clock well-known 'quoted' end- -start rock'n'roll\n"),
            [
                "Don't",
                "o’clock",
                "well-known",
                "quoted",
                "end",
                "start",
                "rock'n'roll"
            ]
        );
    }

    #[test]
    fn possessives_are_looked_up_without_their_ending() {
        assert_eq!(
            found("Mikey's coat, Mikey’s hat, Holmes' pipe.\n"),
            [
                ("Mikey's", "Mikey".into()),
                ("coat", "coat".into()),
                ("Mikey’s", "Mikey".into()),
                ("hat", "hat".into()),
                ("Holmes", "Holmes".into()),
                ("pipe", "pipe".into()),
            ]
        );
    }

    #[test]
    fn capitals_digits_and_underscores_are_not_checked() {
        assert_eq!(
            looked_up("NASA and I saw mp3 files in snake_case, R2D2 too. A OK.\n"),
            ["and", "I", "saw", "files", "in", "too", "A"]
        );
    }

    #[test]
    fn code_is_not_checked() {
        assert_eq!(
            looked_up(
                "Run `cargo tset` now.\n\n```\nfn mian() {}\n```\n\n- item\n\n  ```\n  nested cdoe\n  ```\n"
            ),
            ["Run", "now", "item"]
        );
    }

    #[test]
    fn addresses_are_not_checked() {
        assert_eq!(
            looked_up(
                "See https://exmaple.com/pathh, www.exmaple.org or me@exmaple.com and <https://autoo.link>.\n"
            ),
            ["See", "or", "and"]
        );
    }

    #[test]
    fn link_text_is_checked_but_not_where_it_leads() {
        assert_eq!(
            looked_up("A [nice lnik](https://exmaple.com) and [[Mikey|the boy]].\n"),
            ["A", "nice", "lnik", "and", "the", "boy"]
        );
    }

    #[test]
    fn non_prose_blocks_are_not_checked() {
        let source = "---\ntitel: Front\n---\n\nWord.\n\n<div>\nhtmll\n</div>\n\n[ref]: https://x.org \"Titel\"\n\n![alt txet](pic.png)\n";
        assert_eq!(looked_up(source), ["Word"]);
    }

    #[test]
    fn notes_lists_quotes_and_tables_are_checked() {
        let source = "Text[^1].\n\n> Quoted wrod.\n\n> [!NOTE]\n> Noted.\n\n| Head |\n|---|\n| Cell |\n\n1. Item\n\n[^1]: The note.\n";
        assert_eq!(
            looked_up(source),
            [
                "Text", "Quoted", "wrod", "Noted", "Head", "Cell", "Item", "The", "note"
            ]
        );
    }

    #[test]
    fn inline_html_tags_are_not_checked() {
        assert_eq!(
            looked_up("Some <span class=\"clss\">text</span> here<br>now.\n"),
            ["Some", "text", "here", "now"]
        );
    }

    #[test]
    fn accented_and_other_scripts() {
        assert_eq!(
            looked_up("Café naïve cafe\u{301} Здравствуйте 東京\n"),
            ["Café", "naïve", "cafe\u{301}", "Здравствуйте", "東京"]
        );
    }
}
