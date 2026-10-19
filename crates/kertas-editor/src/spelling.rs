//! The spellcheckers documents share (#141): one per dictionary, loaded
//! once in the background, and the misspelled words in a text, found in
//! the background too, so typing never waits.
//!
//! Each checker remembers the misspelled words of the blocks it has
//! checked, by their source, so checking again after an edit only looks
//! at the blocks that changed.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::future::Shared;
use gpui::{App, Global, Task};
use kertas_core::blocks::Syntax;
use kertas_core::dictionaries::{self, Speller};
use kertas_core::model::Model;
use kertas_core::spelling;

use crate::app_settings;

/// A dictionary, loaded, and what it found in the blocks it has checked.
pub struct Checker {
    speller: Speller,
    /// Each block's misspelled words, by its source.
    blocks: Mutex<HashMap<String, BlockWords>>,
}

/// A block's misspelled words: their ranges in it, and the words looked
/// up.
type BlockWords = Vec<(Range<usize>, String)>;

/// How many blocks a checker remembers before starting again.
const REMEMBERED_BLOCKS: usize = 20_000;

/// A checker being loaded, or loaded: `None` if the dictionary couldn't
/// be read.
pub type Loading = Shared<Task<Option<Arc<Checker>>>>;

/// The checkers loaded or being loaded, by dictionary language.
#[derive(Default)]
struct Checkers(HashMap<String, Loading>);

impl Global for Checkers {}

/// The checker for `language`, as it loads. `None` if there's no
/// dictionary for it.
pub fn checker(language: &str, cx: &mut App) -> Option<Loading> {
    let user = app_settings::dictionaries(cx);
    let bundled;
    let available = if user.is_empty() {
        // A host without Kertas's settings has Kertas's dictionaries.
        bundled = dictionaries::bundled();
        &bundled[..]
    } else {
        user
    };
    let dictionary = dictionaries::find(available, language)?.clone();
    let checkers = cx.default_global::<Checkers>();
    if let Some(loading) = checkers.0.get(&dictionary.tag) {
        return Some(loading.clone());
    }
    let tag = dictionary.tag.clone();
    let loading = cx
        .background_executor()
        .spawn(async move {
            dictionary.load().ok().map(|speller| {
                Arc::new(Checker {
                    speller,
                    blocks: Mutex::default(),
                })
            })
        })
        .shared();
    cx.default_global::<Checkers>()
        .0
        .insert(tag, loading.clone());
    Some(loading)
}

impl Checker {
    /// The misspelled words in `text`, read with `syntax`, as their ranges
    /// in it, in order. Slow for a long text the first time, so run in the
    /// background.
    pub fn misspelled(&self, text: &str, syntax: Syntax) -> Vec<Range<usize>> {
        let model = Model::new_with(text, syntax);
        let mut blocks = self.blocks.lock().unwrap_or_else(|err| err.into_inner());
        if blocks.len() > REMEMBERED_BLOCKS {
            blocks.clear();
        }
        let mut found = Vec::new();
        for item in &model.items {
            let source = &model.text[item.block.clone()];
            let words = blocks.entry(source.to_owned()).or_insert_with(|| {
                spelling::block_words(&item.rendered, 0)
                    .into_iter()
                    .filter(|word| !self.speller.check(&word.word))
                    .map(|word| (word.range, word.word))
                    .collect()
            });
            let start = item.block.start;
            found.extend(
                words
                    .iter()
                    .map(|(range, _)| start + range.start..start + range.end),
            );
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Checker {
        let english = dictionaries::bundled().remove(0);
        Checker {
            speller: english.load().unwrap(),
            blocks: Mutex::default(),
        }
    }

    #[test]
    fn finds_misspelled_words_where_they_are() {
        let text = "# Teh title\n\nA *wrod* in `cdoe`, and colour.\n";
        let found: Vec<&str> = checker()
            .misspelled(text, Syntax::default())
            .into_iter()
            .map(|range| &text[range])
            .collect();
        // The first bundled dictionary is en-US.
        assert_eq!(found, ["Teh", "wrod", "colour"]);
    }

    #[test]
    fn remembers_blocks_wherever_they_move() {
        let checker = checker();
        let first = checker.misspelled("Fine.\n\nA wrod.\n", Syntax::default());
        assert_eq!(first, [9..13]);
        assert_eq!(checker.blocks.lock().unwrap().len(), 2);
        let moved = checker.misspelled("A new paragraph.\n\nFine.\n\nA wrod.\n", Syntax::default());
        assert_eq!(moved, [27..31]);
        assert_eq!(checker.blocks.lock().unwrap().len(), 3);
    }
}
