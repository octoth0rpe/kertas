//! Spelling dictionaries (#140): the English ones Kertas comes with, and
//! any Hunspell dictionaries the user puts in the `dictionaries` folder
//! next to the settings file, as a pair of files named after their
//! language: `fr.aff` and `fr.dic`.
//!
//! The `spelling_language` setting names a dictionary by its language
//! tag, ignoring case. A tag with a region, such as `fr-CA`, falls back to
//! a dictionary for the language alone, `fr`. A user's dictionary for a
//! language Kertas comes with replaces Kertas's.

use std::path::{Path, PathBuf};

/// A dictionary: its language, its name in the settings window, and
/// where its files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dictionary {
    /// Its language tag, as in `en-US`.
    pub tag: String,
    /// Its name in the settings window, as in "English (US)".
    pub name: String,
    source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Bundled {
        aff: &'static str,
        dic: &'static str,
    },
    Files {
        aff: PathBuf,
        dic: PathBuf,
    },
}

/// The language Kertas checks unless told otherwise.
pub const DEFAULT_LANGUAGE: &str = "en-US";

/// The dictionaries Kertas comes with, from SCOWL.
pub fn bundled() -> Vec<Dictionary> {
    [
        (
            "en-US",
            "English (US)",
            include_str!("builtin/dictionaries/en-US.aff"),
            include_str!("builtin/dictionaries/en-US.dic"),
        ),
        (
            "en-GB",
            "English (UK)",
            include_str!("builtin/dictionaries/en-GB.aff"),
            include_str!("builtin/dictionaries/en-GB.dic"),
        ),
    ]
    .into_iter()
    .map(|(tag, name, aff, dic)| Dictionary {
        tag: tag.to_owned(),
        name: name.to_owned(),
        source: Source::Bundled { aff, dic },
    })
    .collect()
}

/// The dictionaries in `folder`: each `.aff` file with a `.dic` file of
/// the same name. A missing folder has none.
pub fn read_folder(folder: &Path) -> Vec<Dictionary> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<Dictionary> = entries
        .filter_map(|entry| {
            let aff = entry.ok()?.path();
            if !aff
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("aff"))
            {
                return None;
            }
            let dic = aff.with_extension("dic");
            let tag = aff.file_stem()?.to_str()?.to_owned();
            (dic.is_file() && is_language_tag(&tag)).then(|| Dictionary {
                name: tag.clone(),
                tag,
                source: Source::Files { aff, dic },
            })
        })
        .collect();
    found.sort_by_key(|dictionary| dictionary.tag.to_lowercase());
    found
}

/// Kertas's dictionaries and the user's, a user's replacing Kertas's for
/// the same language, in name order.
pub fn combine(bundled: Vec<Dictionary>, user: Vec<Dictionary>) -> Vec<Dictionary> {
    let mut all: Vec<Dictionary> = bundled
        .into_iter()
        .filter(|dictionary| {
            !user
                .iter()
                .any(|mine| mine.tag.eq_ignore_ascii_case(&dictionary.tag))
        })
        .collect();
    all.extend(user);
    all.sort_by_key(|dictionary| dictionary.name.to_lowercase());
    all
}

/// The dictionary for `tag` among `dictionaries`: the one for exactly
/// that language, or else for its language without the region.
pub fn find<'a>(dictionaries: &'a [Dictionary], tag: &str) -> Option<&'a Dictionary> {
    let exact = |tag: &str| {
        dictionaries
            .iter()
            .find(|dictionary| dictionary.tag.eq_ignore_ascii_case(tag))
    };
    exact(tag).or_else(|| exact(tag.split('-').next()?))
}

/// Whether `tag` is shaped like a BCP 47 language tag: a language of 2 to
/// 8 letters, then subtags of 1 to 8 letters or digits, joined by `-`.
/// `_` is taken for `-`, as Hunspell's file names use it.
pub fn is_language_tag(tag: &str) -> bool {
    let mut subtags = tag.split(['-', '_']);
    let language = subtags.next().unwrap_or_default();
    (2..=8).contains(&language.len())
        && language.chars().all(|c| c.is_ascii_alphabetic())
        && subtags.all(|subtag| {
            (1..=8).contains(&subtag.len()) && subtag.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

impl Dictionary {
    /// Reads the dictionary, ready to check words. Slow-ish (about 5 ms
    /// for English, ten times that in a debug build), so done once, off
    /// the main thread.
    pub fn load(&self) -> Result<Speller, String> {
        let (aff, dic) = match &self.source {
            Source::Bundled { aff, dic } => ((*aff).to_owned(), (*dic).to_owned()),
            Source::Files { aff, dic } => {
                let read = |path: &Path| {
                    std::fs::read_to_string(path)
                        .map_err(|err| format!("{}: {err}", path.display()))
                };
                (read(aff)?, read(dic)?)
            }
        };
        spellbook::Dictionary::new(&aff, &dic)
            .map(Speller)
            .map_err(|err| format!("the {} dictionary can't be read: {err}", self.tag))
    }
}

/// A loaded dictionary.
pub struct Speller(spellbook::Dictionary);

impl Speller {
    /// Whether `word` is spelled right. A hyphenated word is, if it's in
    /// the dictionary whole or each of its parts is.
    pub fn check(&self, word: &str) -> bool {
        self.0.check(word)
            || (word.contains(['-', '‐'])
                && word
                    .split(['-', '‐'])
                    .all(|part| !part.is_empty() && self.0.check(part)))
    }

    /// Words `word` may have been meant as, best first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut suggestions = Vec::new();
        self.0.suggest(word, &mut suggestions);
        suggestions
    }

    /// Makes `word` known, as it's written.
    pub fn add(&mut self, word: &str) {
        // Only a malformed word (empty, say) fails, and it's then unknown.
        self.0.add(word).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_comes_bundled() {
        let all = combine(bundled(), Vec::new());
        let names: Vec<&str> = all.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["English (UK)", "English (US)"]);
        assert_eq!(find(&all, "en-us").unwrap().tag, "en-US");
        assert_eq!(find(&all, "EN-GB").unwrap().tag, "en-GB");
        assert!(find(&all, "en-AU").is_none());
        assert!(find(&all, "fr").is_none());
    }

    #[test]
    fn checking_words() {
        let us = find(&bundled(), "en-US").unwrap().load().unwrap();
        for word in ["colour", "Mikey", "teh"] {
            assert!(!us.check(word), "{word}");
        }
        for word in ["color", "don’t", "well-known", "cat-flap", "London"] {
            assert!(us.check(word), "{word}");
        }
        assert!(!us.check("cat-flep"));
        let gb = find(&bundled(), "en-GB").unwrap().load().unwrap();
        assert!(gb.check("colour"));
        assert!(us.suggest("teh").contains(&"the".to_owned()));
        let mut us = us;
        us.add("Mikey");
        assert!(us.check("Mikey"));
    }

    #[test]
    fn a_users_dictionaries() {
        let folder =
            std::env::temp_dir().join(format!("kertas-dictionaries-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for (name, text) in [
            ("fr.aff", "SET UTF-8\n"),
            ("fr.dic", "1\nbonjour\n"),
            ("en-US.aff", "SET UTF-8\n"),
            ("en-US.dic", "1\nhowdy\n"),
            ("lonely.aff", ""),
            ("notes.txt", ""),
        ] {
            std::fs::write(folder.join(name), text).unwrap();
        }
        let user = read_folder(&folder);
        let tags: Vec<&str> = user.iter().map(|d| d.tag.as_str()).collect();
        assert_eq!(tags, ["en-US", "fr"]);
        let all = combine(bundled(), user);
        let names: Vec<&str> = all.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["en-US", "English (UK)", "fr"]);
        // The user's en-US replaces Kertas's; fr-CA falls back to fr.
        let mine = find(&all, "en-US").unwrap().load().unwrap();
        assert!(mine.check("howdy") && !mine.check("color"));
        let french = find(&all, "fr-CA").unwrap().load().unwrap();
        assert!(french.check("bonjour"));
        assert!(read_folder(&folder.join("missing")).is_empty());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn language_tags() {
        for tag in ["en", "en-US", "en_GB", "zh-Hant-TW", "es-419"] {
            assert!(is_language_tag(tag), "{tag}");
        }
        for tag in ["", "e", "en-", "123", "en US", "english!"] {
            assert!(!is_language_tag(tag), "{tag}");
        }
    }
}
