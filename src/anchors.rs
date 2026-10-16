//! Where `#name` links lead in a document (#91): its headings, named the
//! way GitHub names them.

use std::collections::HashMap;

use crate::model::Model;
use crate::rendered::RenderedBlock;

/// A heading's anchor, as GitHub makes it: lowercase, punctuation removed
/// (letters, numbers, `_` and `-` are kept), and spaces turned into `-`.
pub fn slug(heading: &str) -> String {
    heading
        .chars()
        .filter(|&c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The anchors in `model`, in document order, with the index of the item
/// each one is in. Repeated headings get `-1`, `-2` and so on, as on GitHub.
pub fn anchors(model: &Model) -> Vec<(String, usize)> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchors = Vec::new();
    for (index, item) in model.items.iter().enumerate() {
        let mut headings = Vec::new();
        collect_headings(&item.rendered, &mut headings);
        for heading in headings {
            let slug = slug(&heading);
            let count = seen.entry(slug.clone()).or_default();
            let anchor = match *count {
                0 => slug,
                n => format!("{slug}-{n}"),
            };
            *count += 1;
            anchors.push((anchor, index));
        }
    }
    anchors
}

/// The index of the item holding the anchor `name`, if there is one. An
/// exact match wins; otherwise case is ignored, so `#Setup` finds `setup`.
pub fn find(model: &Model, name: &str) -> Option<usize> {
    let anchors = anchors(model);
    anchors
        .iter()
        .find(|(anchor, _)| anchor == name)
        .or_else(|| {
            let name = name.to_lowercase();
            anchors.iter().find(|(anchor, _)| *anchor == name)
        })
        .map(|(_, index)| *index)
}

/// The text of every heading in `block`, including headings in quotes and
/// lists.
fn collect_headings(block: &RenderedBlock, out: &mut Vec<String>) {
    match block {
        RenderedBlock::Heading { text, .. } => out.push(text.text.clone()),
        RenderedBlock::List { items, .. } => {
            for child in items.iter().flat_map(|item| &item.children) {
                collect_headings(child, out);
            }
        }
        RenderedBlock::BlockQuote(children) | RenderedBlock::Alert { children, .. } => {
            for child in children {
                collect_headings(child, out);
            }
        }
        _ => {}
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
}
