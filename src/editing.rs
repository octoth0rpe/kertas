//! Markdown-aware editing: what Enter and Shift-Enter insert, indenting
//! list items and code, and where the caret goes when moving to a block.
//! See the Enter addendum in `docs/planning/caret-and-reveal.md`.
//!
//! These are pure functions of the source text, like [`crate::movement`].

use std::ops::Range;

use crate::blocks::BlockKind;
use crate::movement::Selection;

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
        BlockKind::CodeBlock { .. } => {
            // Keep the line's indentation.
            let indent = &before_caret[..before_caret.len() - before_caret.trim_start().len()];
            return newline(indent);
        }
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
    /// The end of the bullet or number and its delimiter.
    token_end: usize,
    /// Where the item's content starts, after the marker and its spaces
    /// (a task box is part of the content).
    content: usize,
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
        let mut marker = format!("{} ", self.next_token());
        if self.task {
            marker.push_str("[ ] ");
        }
        marker
    }

    /// The bullet, or the next number and its delimiter, for the next item.
    fn next_token(&self) -> String {
        match self.kind {
            MarkerKind::Bullet(bullet) => bullet.to_string(),
            MarkerKind::Ordered { number, delimiter } => format!("{}{delimiter}", number + 1),
        }
    }

    /// The token that starts a new list of this kind.
    fn first_token(&self) -> String {
        match self.kind {
            MarkerKind::Bullet(bullet) => bullet.to_string(),
            MarkerKind::Ordered { delimiter, .. } => format!("1{delimiter}"),
        }
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

        let token_end = index;
        index = spaces(index);
        let content = index;
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
            token_end,
            content,
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

/// An edit made in several places at once, applied as one replacement so
/// it's one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    /// The selection afterwards: the same text as before stays selected.
    pub selection: Selection,
}

/// Indents (or outdents) with Tab, Shift-Tab, ⌘] or ⌘[ in a block of
/// `kind` spanning `block`. Returns `None` if there's nothing to indent.
///
/// - In a list, the item at the caret moves one level in or out, with its
///   nested items. Indenting nests it under the item before it, and an
///   ordered item starts the nested list at 1 (or continues one that's
///   already there). Outdenting makes it the next item of its parent.
/// - In a code block, `linewise` indents the selected lines; otherwise Tab
///   inserts an indent at the caret, unless several lines are selected.
///   An indent is a tab, or spaces if the block already indents with
///   spaces.
pub fn indent(
    text: &str,
    kind: BlockKind,
    block: Range<usize>,
    selection: Selection,
    outdent: bool,
    linewise: bool,
) -> Option<Edit> {
    match kind {
        BlockKind::List { .. } | BlockKind::BlockQuote => {
            indent_list_item(text, block, selection, outdent)
        }
        BlockKind::CodeBlock { .. } => indent_code(text, block, selection, outdent, linewise),
        _ => None,
    }
}

fn indent_list_item(
    text: &str,
    block: Range<usize>,
    selection: Selection,
    outdent: bool,
) -> Option<Edit> {
    let lines = lines_in(text, block);
    let caret = selection.range().start;
    let current = lines.iter().rposition(|line| line.start <= caret)?;
    let prefixes: Vec<Prefix> = lines
        .iter()
        .map(|line| Prefix::parse(&text[line.clone()]))
        .collect();
    let marker = |i: usize| prefixes[i].marker.as_ref();
    let blank = |i: usize| {
        marker(i).is_none()
            && text[lines[i].start + prefixes[i].quote_end..lines[i].end]
                .trim()
                .is_empty()
    };
    // Columns are visual (a tab reaches the next multiple of 4, as in
    // CommonMark), counted after any quote markers.
    let after_quotes = |i: usize, end: usize| {
        let line = &text[lines[i].clone()];
        width(&line[prefixes[i].quote_end..end])
    };
    let column = |i: usize| {
        let start = marker(i).map_or(prefixes[i].end, |marker| marker.start);
        after_quotes(i, start)
    };
    let content_column = |i: usize| marker(i).map(|marker| after_quotes(i, marker.content));
    // A line's indentation after its quote markers: where it starts and
    // its text.
    let indentation = |i: usize| {
        let start = lines[i].start + prefixes[i].quote_end;
        let rest = &text[start..lines[i].end];
        let indent = &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()];
        (start, indent)
    };

    // The item the caret is in: its own line, or the nearest item above
    // whose content the caret's line is indented under.
    let item = (0..=current).rev().find(|&i| {
        !blank(i)
            && (i == current && marker(i).is_some()
                || content_column(i).is_some_and(|content| content <= column(current)))
    })?;
    let item_column = column(item);
    // The item's nested lines: those indented further.
    let mut last = item;
    for i in item + 1..lines.len() {
        if blank(i) {
            continue;
        }
        if column(i) <= item_column {
            break;
        }
        last = i;
    }
    let item_marker = marker(item)?;
    let marker_token =
        lines[item].start + item_marker.start..lines[item].start + item_marker.token_end;
    let mut patches = Vec::new();

    let new_token = if outdent {
        if item_column == 0 {
            return None;
        }
        let parent = (0..item)
            .rev()
            .find(|&i| !blank(i) && marker(i).is_some() && column(i) < item_column);
        let delta = item_column - parent.map_or(0, column);
        for i in (item..=last).filter(|&i| !blank(i)) {
            let (start, indent) = indentation(i);
            let narrower = narrow(indent, width(indent).saturating_sub(delta));
            patches.push(Patch {
                at: start,
                remove: indent.len(),
                insert: narrower,
            });
        }
        parent.and_then(marker).map(Marker::next_token)
    } else {
        // Nest under the previous item at the same level.
        let mut sibling = None;
        for i in (0..item).rev() {
            if blank(i) {
                continue;
            }
            if column(i) < item_column {
                break;
            }
            if column(i) == item_column && marker(i).is_some() {
                sibling = Some(i);
                break;
            }
        }
        let sibling = sibling?;
        let new_column = content_column(sibling)?;
        let delta = new_column - item_column;
        for i in (item..=last).filter(|&i| !blank(i)) {
            // Add spaces after any existing indentation, keeping its tabs.
            let (start, indent) = indentation(i);
            patches.push(Patch::insert(start + indent.len(), " ".repeat(delta)));
        }
        // Continue a nested list that's already there, or start one.
        let nested = (sibling + 1..item)
            .rev()
            .find(|&i| !blank(i) && marker(i).is_some() && column(i) == new_column);
        Some(match nested.and_then(marker) {
            Some(nested) => nested.next_token(),
            None => item_marker.first_token(),
        })
    };
    if let Some(token) = new_token
        && token != text[marker_token.clone()]
    {
        patches.push(Patch {
            at: marker_token.start,
            remove: marker_token.len(),
            insert: token,
        });
    }
    combine(text, patches, selection)
}

fn indent_code(
    text: &str,
    block: Range<usize>,
    selection: Selection,
    outdent: bool,
    linewise: bool,
) -> Option<Edit> {
    let lines = lines_in(text, block);
    let is_fence = |line: &Range<usize>| {
        let line = text[line.clone()].trim_start();
        line.starts_with("```") || line.starts_with("~~~")
    };
    // The code lines, without the fences.
    let mut code = 0..lines.len();
    if lines.first().is_some_and(is_fence) {
        code.start = 1;
        if lines.len() > 1 && lines.last().is_some_and(is_fence) {
            code.end -= 1;
        }
    }
    let code_lines = &lines[code.clone()];

    let leading = |line: &Range<usize>| {
        text[line.clone()]
            .bytes()
            .take_while(|&byte| byte == b' ')
            .count()
    };
    let unit = if code_lines
        .iter()
        .any(|line| text[line.clone()].starts_with('\t'))
    {
        "\t".to_owned()
    } else {
        // The indent width is what all indentation is a multiple of: 2 or
        // 4 (or 3), since one level alone can't tell 4 from 8.
        let width = code_lines
            .iter()
            .map(leading)
            .filter(|&count| count > 0)
            .reduce(gcd);
        match width {
            Some(width) if width % 4 == 0 => " ".repeat(4),
            Some(width) if width % 2 == 0 => " ".repeat(2),
            Some(3) => " ".repeat(3),
            Some(_) => " ".repeat(4),
            None => "\t".to_owned(),
        }
    };

    let range = selection.range();
    let line_of = |offset: usize| lines.iter().rposition(|line| line.start <= offset);
    let first = line_of(range.start)?;
    let mut last = line_of(range.end)?;
    if last > first && lines[last].start == range.end {
        last -= 1;
    }
    let selected = first.max(code.start)..(last + 1).min(code.end);
    if selected.is_empty() {
        return None;
    }

    let mut patches = Vec::new();
    if !outdent && !linewise && first == last {
        patches.push(Patch {
            at: range.start,
            remove: range.len(),
            insert: unit,
        });
        // Typing replaces the selection, so the caret goes after the indent.
        let edit = combine(text, patches, selection)?;
        let caret = edit.range.start + edit.text.len();
        return Some(Edit {
            selection: Selection::caret(caret),
            ..edit
        });
    }
    for line in &lines[selected.clone()] {
        if outdent {
            let width = if text[line.clone()].starts_with('\t') {
                1
            } else {
                leading(line).min(unit.len().max(if unit == "\t" { 4 } else { 0 }))
            };
            patches.push(Patch::remove(line.start..line.start + width));
        } else if selected.len() == 1 || !text[line.clone()].trim().is_empty() {
            patches.push(Patch::insert(line.start, unit.clone()));
        }
    }
    combine(text, patches, selection)
}

/// The visual width of leading whitespace: a tab reaches the next multiple
/// of 4, as in CommonMark.
fn width(indent: &str) -> usize {
    indent.chars().fold(0, |column, character| match character {
        '\t' => column + 4 - column % 4,
        _ => column + 1,
    })
}

/// `indent` cut down to `target` columns: its leading tabs and spaces that
/// fit, then spaces.
fn narrow(indent: &str, target: usize) -> String {
    let mut kept = String::new();
    for character in indent.chars() {
        let mut candidate = kept.clone();
        candidate.push(character);
        if width(&candidate) > target {
            break;
        }
        kept = candidate;
    }
    let padding = target - width(&kept);
    kept + &" ".repeat(padding)
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The lines in `range`, without their line endings.
fn lines_in(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = range.start;
    loop {
        let end = line_end(text, start).min(range.end);
        lines.push(start..end);
        match text[end..range.end].find('\n') {
            Some(newline) => start = end + newline + 1,
            None => break,
        }
    }
    lines
}

/// One replacement within a larger edit.
#[derive(Debug)]
struct Patch {
    at: usize,
    remove: usize,
    insert: String,
}

impl Patch {
    fn insert(at: usize, text: String) -> Self {
        Self {
            at,
            remove: 0,
            insert: text,
        }
    }

    fn remove(range: Range<usize>) -> Self {
        Self {
            at: range.start,
            remove: range.len(),
            insert: String::new(),
        }
    }
}

/// Combines `patches` into one edit, and moves `selection` with the text.
fn combine(text: &str, mut patches: Vec<Patch>, selection: Selection) -> Option<Edit> {
    patches.retain(|patch| patch.remove > 0 || !patch.insert.is_empty());
    // At the same position, insertions come before replacements.
    patches.sort_by_key(|patch| (patch.at, patch.remove));
    let start = patches.first()?.at;
    let end = patches.iter().map(|patch| patch.at + patch.remove).max()?;
    let mut replacement = String::new();
    let mut position = start;
    for patch in &patches {
        replacement.push_str(&text[position..patch.at]);
        replacement.push_str(&patch.insert);
        position = patch.at + patch.remove;
    }
    replacement.push_str(&text[position..end]);

    // An insertion right at the caret goes before it. A selection that
    // starts there grows to include it, so whole selected lines stay
    // selected.
    let range = selection.range();
    let map = |offset: usize| {
        let mut shifted = offset as isize;
        for patch in &patches {
            let grows = !range.is_empty() && offset == range.start && patch.at == offset;
            if offset >= patch.at + patch.remove && !grows {
                shifted += patch.insert.len() as isize - patch.remove as isize;
            } else if offset > patch.at {
                // Inside replaced text: move to the end of its replacement.
                return (patch.at as isize + shifted - offset as isize) as usize
                    + patch.insert.len();
            }
        }
        shifted as usize
    };
    Some(Edit {
        range: start..end,
        text: replacement,
        selection: Selection {
            anchor: map(selection.anchor),
            head: map(selection.head),
        },
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

    /// Indents at the selection marked by `[` and `]` (or the caret `|`),
    /// returning the result with the selection marked the same way.
    #[track_caller]
    fn indent_at(text: &str, kind: BlockKind, outdent: bool, linewise: bool) -> String {
        let (text, selection) = match text.find('|') {
            Some(caret) => (text.replace('|', ""), Selection::caret(caret)),
            None => {
                let anchor = text.find('[').unwrap();
                let head = text.find(']').unwrap() - 1;
                (text.replace(['[', ']'], ""), Selection { anchor, head })
            }
        };
        let Some(edit) = indent(&text, kind, 0..text.len(), selection, outdent, linewise) else {
            return "unchanged".to_owned();
        };
        let mut result = text.clone();
        result.replace_range(edit.range, &edit.text);
        let Selection { anchor, head } = edit.selection;
        if anchor == head {
            result.insert(head, '|');
        } else {
            result.insert(head, ']');
            result.insert(anchor, '[');
        }
        result
    }

    #[test]
    fn tab_nests_a_list_item() {
        assert_eq!(indent_at("- a\n- b|", LIST, false, false), "- a\n  - b|");
        assert_eq!(indent_at("- a\n- |b", LIST, false, false), "- a\n  - |b");
        assert_eq!(
            indent_at("- a\n- b|\n  - c\n- d", LIST, false, false),
            "- a\n  - b|\n    - c\n- d",
            "nested items move with it"
        );
        assert_eq!(
            indent_at("- a|\n- b", LIST, false, false),
            "unchanged",
            "first item"
        );
        assert_eq!(
            indent_at("- a\n  - b\n- c|", LIST, false, false),
            "- a\n  - b\n  - c|",
            "joins the nested list"
        );
    }

    #[test]
    fn tab_renumbers_ordered_items() {
        let ordered = BlockKind::List { ordered: true };
        assert_eq!(
            indent_at("1. a\n2. b|", ordered, false, false),
            "1. a\n   1. b|"
        );
        assert_eq!(
            indent_at("1. a\n   1. b\n2. c|", ordered, false, false),
            "1. a\n   1. b\n   2. c|"
        );
        assert_eq!(
            indent_at("1. a\n   1. b|", ordered, true, false),
            "1. a\n2. b|",
            "continues the parent's numbering"
        );
        assert_eq!(
            indent_at("9. a\n10. b|", ordered, false, false),
            "9. a\n   1. b|",
            "a shorter marker"
        );
    }

    #[test]
    fn shift_tab_outdents_a_list_item() {
        assert_eq!(indent_at("- a\n  - b|", LIST, true, false), "- a\n- b|");
        assert_eq!(
            indent_at("- a\n  - b|\n    - c", LIST, true, false),
            "- a\n- b|\n  - c"
        );
        assert_eq!(indent_at("- a|", LIST, true, false), "unchanged");
    }

    #[test]
    fn indenting_from_a_continuation_line() {
        assert_eq!(
            indent_at("- a\n- b\n  more|", LIST, false, false),
            "- a\n  - b\n    more|"
        );
    }

    #[test]
    fn list_items_in_quotes() {
        let quote = BlockKind::BlockQuote;
        assert_eq!(
            indent_at("> - a\n> - b|", quote, false, false),
            "> - a\n>   - b|"
        );
        assert_eq!(
            indent_at("> - a\n>   - b|", quote, true, false),
            "> - a\n> - b|"
        );
        assert_eq!(indent_at("> quote|", quote, false, false), "unchanged");
    }

    #[test]
    fn tab_in_code_inserts_an_indent() {
        let code = BlockKind::CodeBlock { fenced: true };
        assert_eq!(
            indent_at("```\nfn x|\n```", code, false, false),
            "```\nfn x\t|\n```"
        );
        assert_eq!(
            indent_at("```\nfn x() {\n  y|\n```", code, false, false),
            "```\nfn x() {\n  y  |\n```",
            "spaces when the block uses them"
        );
        assert_eq!(
            indent_at("|```\nx\n```", code, false, false),
            "unchanged",
            "fence"
        );
    }

    #[test]
    fn code_lines_indent_and_outdent() {
        let code = BlockKind::CodeBlock { fenced: true };
        assert_eq!(
            indent_at("```\n[a\n\nb]\n```", code, false, false),
            "```\n[\ta\n\n\tb]\n```",
            "selected lines, skipping blank ones"
        );
        assert_eq!(
            indent_at("```\n  a|\n  b\n```", code, false, true),
            "```\n    a|\n  b\n```",
            "⌘] indents the caret's line"
        );
        assert_eq!(
            indent_at("```\n  a\n    b|\n```", code, true, false),
            "```\n  a\n  b|\n```",
            "by the block's indent width"
        );
        assert_eq!(
            indent_at("```\n\ta|\n```", code, true, false),
            "```\na|\n```"
        );
        assert_eq!(
            indent_at("```\n        a|\n```", code, true, false),
            "```\n    a|\n```",
            "one level of 8 counts as two of 4"
        );
    }

    #[test]
    fn other_blocks_do_not_indent() {
        assert_eq!(
            indent_at("Para|", BlockKind::Paragraph, false, false),
            "unchanged"
        );
    }

    #[test]
    fn enter_in_code_keeps_indentation() {
        let code = BlockKind::CodeBlock { fenced: true };
        assert_eq!(
            press("```\n    x|\n```", code, false),
            "```\n    x\n    |\n```"
        );
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

    // Edge cases (#38).

    #[test]
    fn tab_indented_list_items_outdent() {
        // A tab before the marker counts as indentation, like spaces.
        assert_eq!(indent_at("- a\n\t- b|", LIST, true, false), "- a\n- b|");
        // Two levels in, by a tab: back to the first level's spaces.
        assert_eq!(
            indent_at("- a\n  - b\n\t  - c|", LIST, true, false),
            "- a\n  - b\n  - c|"
        );
    }

    #[test]
    fn tab_indented_list_items_indent() {
        assert_eq!(
            indent_at("- a\n\t- b\n\t- c|", LIST, false, false),
            "- a\n\t- b\n\t  - c|"
        );
    }

    #[test]
    fn indent_with_crlf() {
        assert_eq!(
            indent_at("- a\r\n- b|", LIST, false, false),
            "- a\r\n  - b|"
        );
        assert_eq!(indent_at("- a\r\n  - b|", LIST, true, false), "- a\r\n- b|");
        let code = BlockKind::CodeBlock { fenced: true };
        assert_eq!(
            indent_at("```\r\n[a\r\nb]\r\n```", code, false, false),
            "```\r\n[\ta\r\n\tb]\r\n```"
        );
    }

    #[test]
    fn enter_in_tables_html_and_front_matter_is_a_plain_newline() {
        for kind in [
            BlockKind::Table,
            BlockKind::HtmlBlock,
            BlockKind::FrontMatter,
        ] {
            assert_eq!(press("a\n- x|", kind, false), "a\n- x\n|", "{kind:?}");
            assert_eq!(press("- x|", kind, true), "- x\n|", "{kind:?}");
        }
    }

    #[test]
    fn content_start_for_indented_code() {
        let code = BlockKind::CodeBlock { fenced: false };
        assert_eq!(content_start("    let x;", 0..10, code), 4);
    }

    #[test]
    fn nested_list_in_a_quote_outdents_once() {
        let quote = BlockKind::BlockQuote;
        assert_eq!(
            indent_at("> - a\n>   - b|", quote, true, false),
            "> - a\n> - b|"
        );
        assert_eq!(indent_at("> - a\n> - b|", quote, true, false), "unchanged");
    }

    #[test]
    fn ordered_markers_up_to_nine_digits() {
        let ordered = BlockKind::List { ordered: true };
        assert_eq!(
            press("123456789. a|", ordered, false),
            "123456789. a\n123456790. |"
        );
        // Ten digits isn't a list marker, so Enter just adds a line.
        assert_eq!(press("1234567890. a|", ordered, false), "1234567890. a\n|");
    }
}
