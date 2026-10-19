//! The spellcheckers documents share (#141): one per dictionary, loaded
//! once in the background, and the misspelled words in a text, found in
//! the background too, so typing never waits.
//!
//! Each checker remembers the misspelled words of the blocks it has
//! checked, by their source, so checking again after an edit only looks
//! at the blocks that changed.
//!
//! Words can be ignored until Kertas quits, or added to the dictionary
//! (#142), which writes them to the user's word list. Either way, every
//! document is checked again.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, RwLock};

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
    speller: RwLock<Speller>,
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

/// The words ignored until Kertas quits, in every language. Documents
/// observe it, and check their spelling again when it changes, as it does
/// when a word is added to the dictionary too.
#[derive(Default, Clone)]
struct Known {
    ignored: Arc<HashSet<String>>,
}

impl Global for Known {}

/// The words ignored until Kertas quits.
pub fn ignored(cx: &App) -> Arc<HashSet<String>> {
    cx.try_global::<Known>()
        .map(|known| known.ignored.clone())
        .unwrap_or_default()
}

/// Has the entity of `cx` do `f` whenever the known words change, as a
/// document checks its spelling again.
pub fn observe<T: 'static>(
    cx: &mut gpui::Context<T>,
    f: impl Fn(&mut T, &mut gpui::Context<T>) + 'static,
) -> gpui::Subscription {
    cx.observe_global::<Known>(f)
}

/// Ignores `word` until Kertas quits.
pub fn ignore(word: &str, cx: &mut App) {
    let mut known = cx.try_global::<Known>().cloned().unwrap_or_default();
    Arc::make_mut(&mut known.ignored).insert(word.to_owned());
    cx.set_global(known);
}

/// Adds `word` to the dictionary, in every language: to the user's word
/// list, if the host has Kertas's settings, and to the checkers loaded.
pub fn add_to_dictionary(word: &str, cx: &mut App) {
    if let Err(problem) = app_settings::add_to_word_list(word, cx) {
        app_settings::report(problem, cx);
    }
    if let Some(checkers) = cx.try_global::<Checkers>() {
        for checker in checkers
            .0
            .values()
            .filter_map(|loading| loading.peek())
            .flatten()
        {
            checker.add(word);
        }
    }
    known_changed(cx);
}

/// Forgets the checkers loaded, as the dictionaries or the word list may
/// have changed, and checks every document again.
pub(crate) fn dictionaries_changed(cx: &mut App) {
    cx.set_global(Checkers::default());
    known_changed(cx);
}

/// Tells the documents observing the known words that they changed.
pub(crate) fn known_changed(cx: &mut App) {
    let known = cx.try_global::<Known>().cloned().unwrap_or_default();
    cx.set_global(known);
}

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
    let word_list = app_settings::word_list(cx);
    let loading = cx
        .background_executor()
        .spawn(async move {
            let mut speller = dictionary.load().ok()?;
            for word in word_list
                .as_deref()
                .map(dictionaries::read_word_list)
                .unwrap_or_default()
            {
                speller.add(&word);
            }
            Some(Arc::new(Checker {
                speller: RwLock::new(speller),
                blocks: Mutex::default(),
            }))
        })
        .shared();
    cx.default_global::<Checkers>()
        .0
        .insert(tag, loading.clone());
    Some(loading)
}

impl Checker {
    /// The misspelled words in `text`, read with `syntax`, as their ranges
    /// in it, in order, but for those `known`, as they're written or, if
    /// they're capitalized, in lowercase. Slow for a long text the first
    /// time, so run in the background.
    pub fn misspelled(
        &self,
        text: &str,
        syntax: Syntax,
        known: &HashSet<String>,
    ) -> Vec<Range<usize>> {
        let is_known = |word: &str| {
            known.contains(word) || {
                let mut chars = word.chars();
                chars.next().is_some_and(|first| {
                    known.contains(&first.to_lowercase().chain(chars).collect::<String>())
                })
            }
        };
        let model = Model::new_with(text, syntax);
        let speller = self.speller.read().unwrap_or_else(|err| err.into_inner());
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
                    .filter(|word| !speller.check(&word.word))
                    .map(|word| (word.range, word.word))
                    .collect()
            });
            let start = item.block.start;
            found.extend(
                words
                    .iter()
                    .filter(|(_, word)| !is_known(word))
                    .map(|(range, _)| start + range.start..start + range.end),
            );
        }
        found
    }

    /// Words `word` may have been meant as, best first. Takes a few
    /// milliseconds.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let speller = self.speller.read().unwrap_or_else(|err| err.into_inner());
        speller.suggest(word)
    }

    /// Makes `word` known, and forgets the blocks where it was misspelled.
    fn add(&self, word: &str) {
        self.speller
            .write()
            .unwrap_or_else(|err| err.into_inner())
            .add(word);
        let mut blocks = self.blocks.lock().unwrap_or_else(|err| err.into_inner());
        for words in blocks.values_mut() {
            words.retain(|(_, misspelled)| misspelled != word);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Checker {
        let english = dictionaries::bundled().remove(0);
        Checker {
            speller: RwLock::new(english.load().unwrap()),
            blocks: Mutex::default(),
        }
    }

    #[test]
    fn finds_misspelled_words_where_they_are() {
        let text = "# Teh title\n\nA *wrod* in `cdoe`, and colour.\n";
        let found: Vec<&str> = checker()
            .misspelled(text, Syntax::default(), &HashSet::new())
            .into_iter()
            .map(|range| &text[range])
            .collect();
        // The first bundled dictionary is en-US.
        assert_eq!(found, ["Teh", "wrod", "colour"]);
    }

    #[test]
    fn remembers_blocks_wherever_they_move() {
        let checker = checker();
        let first = checker.misspelled("Fine.\n\nA wrod.\n", Syntax::default(), &HashSet::new());
        assert_eq!(first, [9..13]);
        assert_eq!(checker.blocks.lock().unwrap().len(), 2);
        let moved = checker.misspelled(
            "A new paragraph.\n\nFine.\n\nA wrod.\n",
            Syntax::default(),
            &HashSet::new(),
        );
        assert_eq!(moved, [27..31]);
        assert_eq!(checker.blocks.lock().unwrap().len(), 3);
    }

    #[test]
    fn ignored_and_added_words_are_not_misspelled() {
        let checker = checker();
        let text = "A wrod and Mikey.\n";
        let none = HashSet::new();
        let found = |ignored: &HashSet<String>| -> Vec<&str> {
            checker
                .misspelled(text, Syntax::default(), ignored)
                .into_iter()
                .map(|range| &text[range])
                .collect()
        };
        assert_eq!(found(&none), ["wrod", "Mikey"]);
        assert_eq!(found(&HashSet::from(["wrod".to_owned()])), ["Mikey"]);
        assert_eq!(found(&HashSet::from(["mikey".to_owned()])), ["wrod"]);
        checker.add("Mikey");
        assert_eq!(found(&none), ["wrod"]);
        assert!(checker.suggest("wrod").contains(&"word".to_owned()));
    }
}
