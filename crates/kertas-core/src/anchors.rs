//! Where `#name` links lead in a document: its headings, named the way
//! GitHub names them (#91), HTML tags' `id`s and `<a name>`s (#96), and
//! footnotes and their references, named `fn-label` and `fnref-label`
//! as on GitHub (#90).

use std::collections::HashMap;

use crate::blocks::normalize_label;
use crate::html_tags;
use crate::inline::InlineText;
use crate::model::Model;
use crate::rendered::RenderedBlock;

/// The prefix GitHub adds to the `id`s in a rendered README. Links that
/// include it still work.
const GITHUB_PREFIX: &str = "user-content-";

/// A heading's anchor, as GitHub makes it: lowercase, punctuation removed
/// (letters, numbers, `_` and `-` are kept), and spaces turned into `-`.
pub fn slug(heading: &str) -> String {
    heading
        .trim()
        .chars()
        .filter(|&c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Something a `#name` link can lead to.
enum Target {
    /// A heading, by its text.
    Heading(String),
    /// An HTML tag's `id`, or an `<a name>`.
    Html(String),
}

/// The anchors in `model`, in document order, with the index of the item
/// each one is in. Repeated headings get `-1`, `-2` and so on, as on GitHub.
pub fn anchors(model: &Model) -> Vec<(String, usize)> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchors = Vec::new();
    for (index, item) in model.items.iter().enumerate() {
        let mut targets = Vec::new();
        collect(&item.rendered, &mut targets);
        for target in targets {
            let anchor = match target {
                Target::Heading(text) => {
                    let slug = slug(&text);
                    let count = seen.entry(slug.clone()).or_default();
                    let anchor = match *count {
                        0 => slug,
                        n => format!("{slug}-{n}"),
                    };
                    *count += 1;
                    anchor
                }
                Target::Html(id) => id,
            };
            anchors.push((anchor, index));
        }
    }
    // The notes are drawn in the rows after the items.
    for (index, note) in model.notes.iter().enumerate() {
        anchors.push((
            format!("fn-{}", normalize_label(&note.label)),
            model.items.len() + index,
        ));
    }
    anchors.extend(
        references(model)
            .into_iter()
            .map(|(anchor, offset)| (anchor, model.item_at(offset))),
    );
    anchors
}

/// The footnote references' anchors, with where each one starts:
/// `fnref-label` for a note's first reference, then `fnref-label-2` and
/// so on.
fn references(model: &Model) -> Vec<(String, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    model
        .footnote_references
        .iter()
        .map(|(label, offset)| {
            let count = seen.entry(label).or_default();
            *count += 1;
            let anchor = match *count {
                1 => format!("fnref-{label}"),
                n => format!("fnref-{label}-{n}"),
            };
            (anchor, *offset)
        })
        .collect()
}

/// The index of the row holding the anchor `name`, if there is one: an
/// item, or past the items, a footnote. The first match in the document
/// wins. An exact match is preferred; otherwise case is ignored, so
/// `#Setup` finds `setup`. GitHub's `user-content-` prefix is optional.
pub fn find(model: &Model, name: &str) -> Option<usize> {
    lookup(&anchors(model), name)
}

/// Where the footnote reference `name` (`fnref-label`) starts in the
/// document, if there is one. Names match as in [`find`].
pub fn find_reference(model: &Model, name: &str) -> Option<usize> {
    lookup(&references(model), name)
}

/// The value for the anchor `name` in `anchors`. See [`find`].
fn lookup(anchors: &[(String, usize)], name: &str) -> Option<usize> {
    let lookup = |name: &str| {
        anchors
            .iter()
            .find(|(anchor, _)| anchor == name)
            .or_else(|| {
                anchors
                    .iter()
                    .find(|(anchor, _)| anchor.to_lowercase() == name.to_lowercase())
            })
            .map(|(_, value)| *value)
    };
    lookup(name).or_else(|| lookup(name.strip_prefix(GITHUB_PREFIX)?))
}

/// The link targets in `block`, in order, including those in quotes,
/// lists and tables.
fn collect(block: &RenderedBlock, out: &mut Vec<Target>) {
    let inline = |text: &InlineText, out: &mut Vec<Target>| {
        out.extend(text.anchors.iter().cloned().map(Target::Html));
    };
    match block {
        RenderedBlock::Heading { text, .. } => {
            out.push(Target::Heading(text.text.clone()));
            inline(text, out);
        }
        RenderedBlock::Paragraph(text) => inline(text, out),
        RenderedBlock::List { items, .. } => {
            for child in items.iter().flat_map(|item| &item.children) {
                collect(child, out);
            }
        }
        RenderedBlock::BlockQuote(children)
        | RenderedBlock::Alert { children, .. }
        | RenderedBlock::FootnoteDefinition { children, .. } => {
            for child in children {
                collect(child, out);
            }
        }
        RenderedBlock::Table(table) => {
            for cell in table.header.iter().chain(table.rows.iter().flatten()) {
                inline(cell, out);
            }
        }
        RenderedBlock::Html(html) => out.extend(
            html_tags::start_tags(html)
                .iter()
                .filter_map(|tag| tag.anchor().map(|id| Target::Html(id.to_owned()))),
        ),
        RenderedBlock::ThematicBreak | RenderedBlock::CodeBlock { .. } | RenderedBlock::Raw(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_github() {
        assert_eq!(slug("Setup"), "setup");
        assert_eq!(slug("Getting Started"), "getting-started");
        assert_eq!(slug("What's new in v0.3?"), "whats-new-in-v03");
        assert_eq!(
            slug("snake_case and kebab-case"),
            "snake_case-and-kebab-case"
        );
        assert_eq!(slug("A  B"), "a--b");
        assert_eq!(slug("Café & Crème"), "café--crème");
        assert_eq!(slug("日本語"), "日本語");
        assert_eq!(slug("--- C++ ---"), "----c----");
    }

    #[test]
    fn anchors_use_the_heading_text_without_formatting() {
        let model = Model::new("# The `kt` *command*\n\nText\n\n## [Links](https://x)\n");
        assert_eq!(
            anchors(&model),
            [("the-kt-command".to_owned(), 0), ("links".to_owned(), 2)]
        );
    }

    #[test]
    fn repeated_headings_are_numbered() {
        let model = Model::new("# Notes\n\n## Notes\n\n### Notes\n");
        assert_eq!(
            anchors(&model),
            [
                ("notes".to_owned(), 0),
                ("notes-1".to_owned(), 1),
                ("notes-2".to_owned(), 2)
            ]
        );
    }

    #[test]
    fn find_returns_the_item() {
        let model = Model::new("# Intro\n\nText\n\n## Setup\n\nMore\n\n> ## Quoted\n");
        assert_eq!(find(&model, "setup"), Some(2));
        assert_eq!(find(&model, "Setup"), Some(2));
        assert_eq!(find(&model, "quoted"), Some(4));
        assert_eq!(find(&model, "missing"), None);
    }

    #[test]
    fn html_anchors() {
        let model = Model::new(
            "# Intro\n\n<a id=\"install\"></a>\nInstall it.\n\n<div id=\"box\">\n\n| <span id=cell>x</span> |\n|---|\n\n- <a name=\"item\"></a>Item\n",
        );
        assert_eq!(find(&model, "install"), Some(1));
        assert_eq!(find(&model, "box"), Some(2));
        assert_eq!(find(&model, "cell"), Some(3));
        assert_eq!(find(&model, "item"), Some(4));
    }

    #[test]
    fn the_first_match_wins() {
        let model = Model::new("<a id=\"setup\"></a>Early\n\n## Setup\n\n<a id=\"intro\"></a>\n");
        assert_eq!(find(&model, "setup"), Some(0));
        let model = Model::new("## Setup\n\n<a id=\"setup\"></a>Late\n");
        assert_eq!(find(&model, "setup"), Some(0));
    }

    #[test]
    fn the_github_prefix_is_optional() {
        let model = Model::new("# Intro\n\n## Setup\n\n<a id=\"x\"></a>\n");
        assert_eq!(find(&model, "user-content-setup"), Some(1));
        assert_eq!(find(&model, "user-content-x"), Some(2));
    }

    #[test]
    fn footnotes_and_their_references() {
        let model =
            Model::new("# Intro\n\nOne[^a] and two[^B].\n\nAgain[^a].\n\n[^b]: Bee.\n[^a]: Ay.\n");
        // The items are the heading, two paragraphs and two definitions;
        // the notes come after them, in the order they're referred to.
        assert_eq!(find(&model, "fn-a"), Some(5));
        assert_eq!(find(&model, "user-content-fn-b"), Some(6));
        assert_eq!(find(&model, "fnref-a"), Some(1));
        assert_eq!(find(&model, "fnref-a-2"), Some(2));
        assert_eq!(
            find_reference(&model, "fnref-b"),
            Some(model.text.find("[^B]").unwrap())
        );
        assert_eq!(
            find_reference(&model, "fnref-a-2"),
            Some(model.text.find("Again").unwrap() + 5)
        );
        assert_eq!(find_reference(&model, "fnref-a-3"), None);
    }
}
