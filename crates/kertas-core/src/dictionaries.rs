//! Spelling dictionaries (#140): the English ones Kertas comes with, and
//! any Hunspell dictionaries the user puts in the `dictionaries` folder
//! next to the settings file, as a pair of files named after their
//! language: `fr.aff` and `fr.dic`.
//!
//! The `spelling_language` setting names a dictionary by its language
//! tag, ignoring case. A tag with a region, such as `fr-CA`, falls back to
//! a dictionary for the language alone, `fr`. A user's dictionary for a
//! language Kertas comes with replaces Kertas's.
//!
//! The words the user adds to the dictionary (#142) are in the same
//! folder, one to a line, in [`WORD_LIST`], and are known in every
//! language.

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

/// The file of the words the user added, in the dictionaries folder.
pub const WORD_LIST: &str = "words.txt";

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
/// that language, or else for its language without the region. A bare
/// language (`en`) with no dictionary of its own takes one of its regions:
/// its home region (`fr-FR`, or `en-US`, Kertas's default language), or
/// else the first by name. A tag with a region never takes another region.
pub fn find<'a>(dictionaries: &'a [Dictionary], tag: &str) -> Option<&'a Dictionary> {
    let exact = |tag: &str| {
        dictionaries
            .iter()
            .find(|dictionary| dictionary.tag.eq_ignore_ascii_case(tag))
    };
    let language = tag.split('-').next()?;
    exact(tag)
        .or_else(|| exact(language))
        .or_else(|| (language.len() == tag.len()).then(|| a_region_of(dictionaries, language))?)
}

/// A dictionary for one of `language`'s regions, its home region first.
fn a_region_of<'a>(dictionaries: &'a [Dictionary], language: &str) -> Option<&'a Dictionary> {
    let home = if language.eq_ignore_ascii_case("en") {
        "US".to_owned()
    } else {
        language.to_ascii_uppercase()
    };
    let region = |dictionary: &Dictionary| {
        let (of, region) = dictionary.tag.split_once('-')?;
        of.eq_ignore_ascii_case(language)
            .then(|| region.to_ascii_uppercase())
    };
    let regional = || {
        dictionaries
            .iter()
            .filter(move |dictionary| region(dictionary).is_some())
    };
    regional()
        .find(|dictionary| region(dictionary).as_deref() == Some(home.as_str()))
        .or_else(|| regional().min_by_key(|dictionary| dictionary.name.to_lowercase()))
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

/// The words in the word list at `path`, one to a line. A missing file
/// has none.
pub fn read_word_list(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Adds `word` to the end of the word list at `path`, creating it and its
/// folder if needed, unless it's already there.
pub fn add_to_word_list(path: &Path, word: &str) -> std::io::Result<()> {
    use std::io::Write;

    if read_word_list(path).iter().any(|known| known == word) {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let newline = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    writeln!(file, "{newline}{word}")
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

/// `word` with its first letter in capitals.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
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

    /// Words `word` may have been meant as, best first. A capitalized
    /// word gets the suggestions for it in lowercase first, capitalized,
    /// as they're better: `Teh` gets `The` before `Eh`.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut suggestions = Vec::new();
        let mut chars = word.chars();
        let capitalized = chars.next().is_some_and(char::is_uppercase)
            && chars.clone().any(char::is_lowercase)
            && !chars.any(char::is_uppercase);
        if capitalized {
            self.0.suggest(&word.to_lowercase(), &mut suggestions);
            suggestions = suggestions.iter().map(|word| capitalize(word)).collect();
        }
        let mut more = Vec::new();
        self.0.suggest(word, &mut more);
        for suggestion in more {
            if !suggestions.contains(&suggestion) {
                suggestions.push(suggestion);
            }
        }
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

    fn dictionary(tag: &str, name: &str) -> Dictionary {
        Dictionary {
            tag: tag.to_owned(),
            name: name.to_owned(),
            source: Source::Bundled { aff: "", dic: "" },
        }
    }

    #[test]
    fn a_bare_language_takes_one_of_its_regions() {
        let all = combine(bundled(), Vec::new());
        assert_eq!(
            find(&all, "en").unwrap().tag,
            "en-US",
            "English's home is the US"
        );
        assert_eq!(find(&all, "EN").unwrap().tag, "en-US");
        let gb = [dictionary("en-GB", "English (UK)")];
        assert_eq!(find(&gb, "en").unwrap().tag, "en-GB", "with no US, another");

        let french = [
            dictionary("fr-CA", "French (Canada)"),
            dictionary("fr-FR", "French (France)"),
            dictionary("fr-BE", "French (Belgium)"),
        ];
        assert_eq!(find(&french, "fr").unwrap().tag, "fr-FR", "the home region");
        let elsewhere = [
            dictionary("pt-PT", "Portuguese (Portugal)"),
            dictionary("pt-BR", "Portuguese (Brazil)"),
        ];
        assert_eq!(find(&elsewhere, "pt").unwrap().tag, "pt-PT");
        let no_home = [
            dictionary("fr-CA", "French (Canada)"),
            dictionary("fr-BE", "French (Belgium)"),
        ];
        assert_eq!(
            find(&no_home, "fr").unwrap().tag,
            "fr-BE",
            "else the first by name"
        );

        // The bare language's own dictionary still comes first, and a
        // region never takes another.
        let both = [
            dictionary("fr-FR", "French (France)"),
            dictionary("fr", "French"),
        ];
        assert_eq!(find(&both, "fr").unwrap().tag, "fr");
        assert!(find(&french, "fr-CH").is_none());
        assert!(find(&french, "frx").is_none());
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
        assert_eq!(us.suggest("teh").first().map(String::as_str), Some("the"));
        assert_eq!(us.suggest("Teh").first().map(String::as_str), Some("The"));
        assert_eq!(us.suggest("Wrod").first().map(String::as_str), Some("Word"));
        assert_eq!(us.suggest("lnik"), ["link"]);
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
    fn the_word_list() {
        let folder = std::env::temp_dir().join(format!("kertas-words-{}", std::process::id()));
        let path = folder.join("dictionaries").join(WORD_LIST);
        assert!(read_word_list(&path).is_empty());
        add_to_word_list(&path, "Mikey").unwrap();
        add_to_word_list(&path, "Mikey").unwrap();
        std::fs::write(&path, "Mikey\n\n  Baskerville  ").unwrap();
        add_to_word_list(&path, "Fulham").unwrap();
        assert_eq!(read_word_list(&path), ["Mikey", "Baskerville", "Fulham"]);
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
