//! Markdown-aware editing: what Enter and Shift-Enter insert, and where the
//! caret goes when moving to a block. See the Enter addendum in
//! `docs/planning/caret-and-reveal.md`.
//!
//! These are pure functions of the source text, like [`crate::movement`].

use std::ops::Range;

use crate::blocks::BlockKind;

/// The edit Enter (or Shift-Enter, if `shift`) makes with `selection`
/// selected in a block of `kind`: the range to replace and its replacement.
/// The caret goes after the replacement.
///
/// - Lists and blockquotes continue with the line's marker. On an item or
///   quote line with nothing after the marker, Enter removes one level:
///   a nested item becomes an item of its parent list, and otherwise the
///   marker is removed and a blank line ends the list or quote.
/// - Shift-Enter inserts a hard line break (`\`) after text in prose, and
///   continues a list item or quote on the next line.
/// - Everything else inserts a plain line break.
pub fn enter(
    text: &str,
    kind: BlockKind,
    selection: Range<usize>,
    shift: bool,
    line_ending: &str,
) -> (Range<usize>, String) {
    let caret = selection.start;
    let line_start = line_start(text, caret);
    let line_end = line_end(text, caret);
    let line = &text[line_start..line_end];
    let before_caret = &line[..caret - line_start];
    let has_text_before = !before_caret.trim().is_empty();
    let hard_break = |continuation: &str| {
        let mut inserted = String::new();
        if has_text_before {
            inserted.push('\\');
        }
        inserted.push_str(line_ending);
        inserted.push_str(continuation);
        (selection.clone(), inserted)
    };
    let newline = |continuation: &str| (selection.clone(), format!("{line_ending}{continuation}"));

    match kind {
        BlockKind::List { .. } | BlockKind::BlockQuote => {}
        BlockKind::Paragraph if shift => return hard_break(""),
        _ => return newline(""),
    }

    let prefix = Prefix::parse(line);
    // A caret inside the marker just inserts a line break.
    if caret - line_start < prefix.end {
        return newline("");
    }
    let quotes = &line[..prefix.quote_end];

    if shift {
        let indent = match &prefix.marker {
            // Line up with the item's content.
            Some(marker) => " ".repeat(prefix.end - marker.indent_start),
            None => line[prefix.quote_end..prefix.end].to_owned(),
        };
        return hard_break(&format!("{quotes}{indent}"));
    }

    let empty = selection.is_empty() && line[prefix.end..].trim().is_empty();
    if empty && prefix.end > 0 {
        // Remove one level of nesting.
        let replacement = match &prefix.marker {
            Some(marker) if marker.indent_start < marker.start => {
                parent_item(text, line_start, &prefix)
            }
            Some(_) if prefix.quote_end > 0 => Some(quotes.to_owned()),
            Some(_) => None,
            None => prefix
                .quote_levels
                .iter()
                .rev()
                .nth(1)
                .map(|&outer| line[..outer].to_owned()),
        };
        let replacement = replacement.unwrap_or_else(|| line_ending.to_owned());
        // A hard break (`\`) on the line before would now end its block,
        // where it shows as a backslash, so remove it.
        let before = &text[..line_start];
        let previous_ending = before
            .strip_suffix(line_ending)
            .or_else(|| before.strip_suffix('\n'))
            .filter(|previous| previous.ends_with('\\'));
        return match previous_ending {
            Some(previous) => (
                previous.len() - 1..line_end,
                format!("{}{replacement}", &before[previous.len()..]),
            ),
            None => (line_start..line_end, replacement),
        };
    }

    let continuation = match &prefix.marker {
        Some(marker) => format!(
            "{quotes}{}{}",
            &line[prefix.quote_end..marker.start],
            marker.next()
        ),
        // A continuation line: keep its indentation.
        None => line[..prefix.end].to_owned(),
    };
    newline(&continuation)
}

/// Where the caret goes when moving to a block: the start of its text,
/// after markers like `## `, `- ` and `> `, or after a code fence.
pub fn content_start(text: &str, block: Range<usize>, kind: BlockKind) -> usize {
    let source = &text[block.clone()];
    let first_line = &source[..source.find(['\r', '\n']).unwrap_or(source.len())];
    let indent = first_line.len() - first_line.trim_start().len();
    let offset = match kind {
        BlockKind::Heading { .. } if first_line.trim_start().starts_with('#') => {
            let after_hashes = first_line.trim_start().trim_start_matches('#');
            first_line.len() - after_hashes.trim_start().len()
        }
        BlockKind::List { .. } | BlockKind::BlockQuote => Prefix::parse(first_line).end,
        BlockKind::CodeBlock { fenced: true } | BlockKind::FrontMatter => match source.find('\n') {
            Some(newline) if newline + 1 < source.len() => newline + 1,
            _ => first_line.len(),
        },
        _ => indent,
    };
    block.start + offset.min(source.len())
}

/// The start of the line containing `offset`.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

/// The end of the line containing `offset`, before its line ending.
fn line_end(text: &str, offset: usize) -> usize {
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    if end > offset && text.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

/// The Markdown container markers at the start of a line.
#[derive(Debug, Default)]
struct Prefix {
    /// The end of each `>` quote marker (and the space after it).
    quote_levels: Vec<usize>,
    /// The end of all quote markers.
    quote_end: usize,
    marker: Option<Marker>,
    /// Where the line's content starts.
    end: usize,
}

/// A list item marker.
#[derive(Debug)]
struct Marker {
    /// The start of the indentation before the marker.
    indent_start: usize,
    start: usize,
    kind: MarkerKind,
    task: bool,
}

#[derive(Debug)]
enum MarkerKind {
    Bullet(char),
    Ordered { number: u64, delimiter: char },
}

impl Marker {
    /// The marker for the next item: the same bullet, or the next number.
    /// Task items get an unchecked box.
    fn next(&self) -> String {
        let mut marker = match self.kind {
            MarkerKind::Bullet(bullet) => format!("{bullet} "),
            MarkerKind::Ordered { number, delimiter } => format!("{}{delimiter} ", number + 1),
        };
        if self.task {
            marker.push_str("[ ] ");
        }
        marker
    }
}

impl Prefix {
    fn parse(line: &str) -> Self {
        let bytes = line.as_bytes();
        let spaces = |mut index: usize| {
            while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
                index += 1;
            }
            index
        };

        let mut prefix = Prefix::default();
        let mut index = spaces(0);
        while index < bytes.len() && bytes[index] == b'>' {
            index += 1;
            if index < bytes.len() && bytes[index] == b' ' {
                index += 1;
            }
            prefix.quote_levels.push(index);
            prefix.quote_end = index;
            index = spaces(index);
        }
        let indent_start = prefix.quote_end;

        let marker_start = index;
        let rest = &line[index..];
        let kind =
            if let Some(bullet) = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+')) {
                index += 1;
                Some(MarkerKind::Bullet(bullet))
            } else {
                let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                match rest.as_bytes().get(digits) {
                    Some(&delimiter @ (b'.' | b')')) if (1..=9).contains(&digits) => {
                        index += digits + 1;
                        Some(MarkerKind::Ordered {
                            number: rest[..digits].parse().unwrap_or(0),
                            delimiter: delimiter as char,
                        })
                    }
                    _ => None,
                }
            };
        // A marker must be followed by a space or the end of the line.
        let kind = kind.filter(|_| index == bytes.len() || bytes[index] == b' ');
        let Some(kind) = kind else {
            prefix.end = marker_start;
            return prefix;
        };

        index = spaces(index);
        let task = ["[ ]", "[x]", "[X]"]
            .iter()
            .any(|task_box| line[index..].starts_with(task_box))
            && matches!(bytes.get(index + 3), None | Some(b' '));
        if task {
            index = spaces(index + 3);
        }
        prefix.marker = Some(Marker {
            indent_start,
            start: marker_start,
            kind,
            task,
        });
        prefix.end = index;
        prefix
    }
}

/// The line that continues the list containing a nested item's line at
/// `line_start`: the parent item's indentation and next marker.
fn parent_item(text: &str, line_start: usize, prefix: &Prefix) -> Option<String> {
    let marker = prefix.marker.as_ref()?;
    let indent = marker.start - marker.indent_start;
    text[..line_start].lines().rev().find_map(|line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            return None;
        }
        let candidate = Prefix::parse(line);
        let parent = candidate.marker.as_ref()?;
        (candidate.quote_end == prefix.quote_end && parent.start - parent.indent_start < indent)
            .then(|| format!("{}{}", &line[..parent.start], parent.next()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: BlockKind = BlockKind::List { ordered: false };

    /// Applies Enter at the `|` in `text`, returning the result with `|` at
    /// the new caret.
    #[track_caller]
    fn press(text: &str, kind: BlockKind, shift: bool) -> String {
        let caret = text.find('|').unwrap();
        let text = text.replace('|', "");
        let (range, inserted) = enter(&text, kind, caret..caret, shift, "\n");
        let caret = range.start + inserted.len();
        let mut result = text.clone();
        result.replace_range(range, &inserted);
        result.insert(caret, '|');
        result
    }

    #[test]
    fn paragraphs_get_plain_newlines() {
        let paragraph = BlockKind::Paragraph;
        assert_eq!(press("Para|", paragraph, false), "Para\n|");
        assert_eq!(press("One |two", paragraph, false), "One \n|two");
        assert_eq!(
            press("# Title|", BlockKind::Heading { level: 1 }, false),
            "# Title\n|"
        );
    }

    #[test]
    fn shift_enter_inserts_a_hard_break_after_text() {
        assert_eq!(press("Line|", BlockKind::Paragraph, true), "Line\\\n|");
        assert_eq!(
            press("|", BlockKind::Paragraph, true),
            "\n|",
            "nothing to break"
        );
        assert_eq!(
            press("# Title|", BlockKind::Heading { level: 1 }, true),
            "# Title\n|"
        );
        assert_eq!(
            press("x|", BlockKind::CodeBlock { fenced: true }, true),
            "x\n|"
        );
    }

    #[test]
    fn lists_continue() {
        assert_eq!(press("- a|", LIST, false), "- a\n- |");
        assert_eq!(press("* a|", LIST, false), "* a\n* |");
        assert_eq!(press("- a|b", LIST, false), "- a\n- |b", "splits the item");
        let ordered = BlockKind::List { ordered: true };
        assert_eq!(press("1. a|", ordered, false), "1. a\n2. |");
        assert_eq!(press("9) a|", ordered, false), "9) a\n10) |");
        assert_eq!(press("- [x] done|", LIST, false), "- [x] done\n- [ ] |");
        assert_eq!(press("- a\n  - b|", LIST, false), "- a\n  - b\n  - |");
    }

    #[test]
    fn empty_items_end_the_list() {
        assert_eq!(press("- a\n- |", LIST, false), "- a\n\n|");
        assert_eq!(press("- a\n- [ ] |", LIST, false), "- a\n\n|");
        assert_eq!(
            press(
                "1. a\n   1. b\n   2. |",
                BlockKind::List { ordered: true },
                false
            ),
            "1. a\n   1. b\n2. |",
            "a nested item moves to the parent list"
        );
    }

    #[test]
    fn caret_before_marker_inserts_a_newline() {
        assert_eq!(press("|- a", LIST, false), "\n|- a");
    }

    #[test]
    fn continuation_lines_keep_their_indentation() {
        assert_eq!(press("- a\n  b|", LIST, false), "- a\n  b\n  |");
    }

    #[test]
    fn shift_enter_continues_the_item() {
        assert_eq!(press("- a|", LIST, true), "- a\\\n  |");
        assert_eq!(
            press("10. a|", BlockKind::List { ordered: true }, true),
            "10. a\\\n    |"
        );
    }

    #[test]
    fn quotes_continue_and_end() {
        let quote = BlockKind::BlockQuote;
        assert_eq!(press("> a|", quote, false), "> a\n> |");
        assert_eq!(press("> a\n> |", quote, false), "> a\n\n|");
        assert_eq!(press("> > a\n> > |", quote, false), "> > a\n> |");
        assert_eq!(press("> - a|", quote, false), "> - a\n> - |");
        assert_eq!(press("> - a\n> - |", quote, false), "> - a\n> |");
        assert_eq!(press("> a|", quote, true), "> a\\\n> |");
    }

    #[test]
    fn ending_after_a_hard_break_removes_it() {
        let quote = BlockKind::BlockQuote;
        assert_eq!(press("> a\\\n> |", quote, false), "> a\n\n|");
        assert_eq!(press("- a\\\n  |", LIST, false), "- a\n\n|");
    }

    #[test]
    fn crlf() {
        let text = "- a\r\n- ";
        let (range, inserted) = enter(text, LIST, 7..7, false, "\r\n");
        assert_eq!((range, inserted.as_str()), (5..7, "\r\n"));
    }

    #[test]
    fn content_starts_after_markers() {
        let start = |text: &str, kind| content_start(text, 0..text.len(), kind);
        assert_eq!(start("## Title", BlockKind::Heading { level: 2 }), 3);
        assert_eq!(start("Title\n===", BlockKind::Heading { level: 1 }), 0);
        assert_eq!(start("- [ ] task\n- b", LIST), 6);
        assert_eq!(start("> quote", BlockKind::BlockQuote), 2);
        assert_eq!(
            start("```rust\ncode\n```", BlockKind::CodeBlock { fenced: true }),
            8
        );
        assert_eq!(start("```", BlockKind::CodeBlock { fenced: true }), 3);
        assert_eq!(start("Para", BlockKind::Paragraph), 0);
        assert_eq!(
            content_start("A\n\n## B", 3..7, BlockKind::Heading { level: 2 }),
            6
        );
    }
}
