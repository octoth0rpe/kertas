//! Underlining misspelled words (#141), checked in the background once
//! typing pauses, in the language the configuration names, and their menu
//! (#142): suggestions, Ignore and Add to Dictionary.
//!
//! A host can steer it (#143): set the language, add words it knows, such
//! as its characters' names, and put its own item in the menu in place of
//! Add for This Document.

use std::rc::Rc;

use gpui::{AppContext, Context, Entity, Hsla, Pixels, Point, SharedString, Subscription, Window};
use kertas_core::rendered::AlertKind;
use kertas_core::spelling::{self, Word};

use super::{DocumentEvent, DocumentView};
use crate::context_menu::{ContextMenu, Dismissed};
use crate::marks::{self, Mark, MarkStyle};

/// The menu of a misspelled word, while it's open.
pub(super) struct SpellingMenu {
    pub(super) menu: Entity<ContextMenu>,
    _dismissed: Subscription,
}

/// What a host set about spelling. It stays as it is when another
/// document is opened.
#[derive(Default)]
pub(super) struct HostSpelling {
    /// The language, in place of the configuration's.
    language: Option<String>,
    /// The words the host knows are spelled right.
    known: Vec<String>,
    /// The label of the host's item in the menu.
    add_word: Option<SharedString>,
}

/// How many suggestions the menu offers at most.
const SUGGESTIONS: usize = 6;

/// The misspelled words, as marks of their own, separate from the
/// host's, drawn with a wavy underline in the theme's caution color.
#[derive(Default)]
pub(super) struct Misspelled {
    pub(super) marks: Rc<Vec<Mark>>,
    /// The text the marks were last moved to, as the host's marks have.
    model: Option<Rc<kertas_core::model::Model>>,
    /// Bumped to drop a check the text has moved on from.
    epoch: usize,
}

impl DocumentView {
    /// Checks spelling in `language`, a language tag naming a dictionary,
    /// in place of the configuration's `spelling_language`, or in that
    /// again with `None`. If there's no dictionary for it, spelling isn't
    /// checked, and it's for the host to say so.
    pub fn set_spelling_language(&mut self, language: Option<String>, cx: &mut Context<Self>) {
        if language != self.host_spelling.language {
            self.host_spelling.language = language;
            self.check_spelling(cx);
        }
    }

    /// Makes `words` known to be spelled right, replacing the words the
    /// host gave before. A capitalized word is known if it's given in
    /// lowercase.
    pub fn set_known_words(&mut self, words: Vec<String>, cx: &mut Context<Self>) {
        if words != self.host_spelling.known {
            self.host_spelling.known = words;
            self.check_spelling(cx);
        }
    }

    /// Puts an item called `label` in a misspelled word's menu, in place
    /// of Add for This Document, which sends [`DocumentEvent::AddWord`]
    /// when it's chosen. The host adds the word to its own words, and
    /// gives them again with [`DocumentView::set_known_words`]. `None`
    /// puts Add for This Document back.
    pub fn set_add_word_item(&mut self, label: Option<SharedString>) {
        self.host_spelling.add_word = label;
    }

    /// The language spelling is checked in.
    fn spelling_language(&self) -> &str {
        self.host_spelling
            .language
            .as_deref()
            .unwrap_or(&self.config.spelling_language)
    }

    /// Checks the document's spelling, in the background, and underlines
    /// what's misspelled once that's done. Spelling isn't checked if the
    /// configuration turns it off, there's no dictionary for its
    /// language, or the document can't be edited.
    pub(super) fn check_spelling(&mut self, cx: &mut Context<Self>) {
        self.misspelled.epoch += 1;
        let epoch = self.misspelled.epoch;
        let model = self
            .open
            .as_ref()
            .filter(|open| {
                self.config.check_spelling && !open.editor.read_only() && self.text_name.is_none()
            })
            .map(|open| open.editor.model().clone());
        let loading = model
            .as_ref()
            .and_then(|_| crate::spelling::checker(self.spelling_language(), cx));
        let (Some(model), Some(loading)) = (model, loading) else {
            if !self.misspelled.marks.is_empty() {
                self.misspelled = Misspelled {
                    epoch,
                    ..Misspelled::default()
                };
                cx.notify();
            }
            return;
        };
        let syntax = self.config.syntax();
        let text = model.text.to_string();
        let mut known = (*crate::spelling::ignored(cx)).clone();
        if let Some(path) = self.document_path().map(std::path::Path::to_owned) {
            known.extend(
                crate::app_settings::document_words(&path, cx)
                    .iter()
                    .cloned(),
            );
        }
        known.extend(self.host_spelling.known.iter().cloned());
        cx.spawn(async move |this, cx| {
            let Some(checker) = loading.await else {
                return;
            };
            let found = cx
                .background_executor()
                .spawn(async move { checker.misspelled(&text, syntax, &known) })
                .await;
            this.update(cx, |view, cx| {
                if view.misspelled.epoch != epoch {
                    return;
                }
                let color = view.misspelling_color();
                let marks = found
                    .into_iter()
                    .map(|range| Mark {
                        range,
                        style: MarkStyle::Wavy(color),
                        id: 0,
                    })
                    .collect();
                view.misspelled.marks = Rc::new(marks);
                view.misspelled.model = Some(model);
                // The text may have changed while it was being checked.
                view.move_misspellings();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens the menu of the misspelled word `mark` underlines, which was
    /// right-clicked or Control-clicked at `position`, once its
    /// suggestions are found.
    pub(crate) fn spelling_menu(
        &mut self,
        mark: &Mark,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        let model = open.editor.model().clone();
        let Some(word) = spelling::word_at(&model, mark.range.start) else {
            return;
        };
        let Some(loading) = crate::spelling::checker(self.spelling_language(), cx) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Some(checker) = loading.await else {
                return;
            };
            let lookup = word.word.clone();
            let suggestions = cx
                .background_executor()
                .spawn(async move { checker.suggest(&lookup) })
                .await;
            this.update_in(cx, |view, window, cx| {
                // Not if the text changed while the suggestions were found.
                if view
                    .open
                    .as_ref()
                    .is_some_and(|open| Rc::ptr_eq(open.editor.model(), &model))
                {
                    view.show_spelling_menu(&model.text, word, suggestions, position, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn show_spelling_menu(
        &mut self,
        text: &str,
        word: Word,
        suggestions: Vec<String>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A suggestion replaces the word but not its possessive ending, or
        // the whole of a word with Markdown inside it.
        let source = &text[word.range.clone()];
        let replaced = if source.starts_with(&word.word) {
            word.range.start..word.range.start + word.word.len()
        } else {
            word.range.clone()
        };
        let view = cx.weak_entity();
        let theme = self.theme();
        let host_item = self.host_spelling.add_word.clone();
        // Words can be added for a document that has a file, in Kertas's
        // settings.
        let document = self
            .document_path()
            .map(std::path::Path::to_owned)
            .filter(|_| crate::app_settings::is_loaded(cx));
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(
                format!("Spelling of {}", word.word),
                position,
                theme,
                window,
                cx,
            );
            for suggestion in suggestions.into_iter().take(SUGGESTIONS) {
                let (view, replaced) = (view.clone(), replaced.clone());
                menu = menu.item(suggestion.clone(), move |_, cx| {
                    view.update(cx, |view, cx| {
                        view.replace_ranges(vec![(replaced.clone(), suggestion.clone())], cx)
                    })
                    .ok();
                });
            }
            let ignored = word.word.clone();
            menu = menu
                .separator()
                .item("Ignore", move |_, cx| crate::spelling::ignore(&ignored, cx));
            if let Some(label) = host_item {
                let (view, added) = (view.clone(), word.word.clone());
                menu = menu.item(label, move |_, cx| {
                    view.update(cx, |_, cx| {
                        cx.emit(DocumentEvent::AddWord {
                            word: added.clone(),
                        })
                    })
                    .ok();
                });
            } else if let Some(path) = document {
                let added = word.word.clone();
                menu = menu.item("Add for This Document", move |_, cx| {
                    if let Err(problem) = crate::app_settings::add_document_word(&path, &added, cx)
                    {
                        crate::app_settings::report(problem, cx);
                    }
                });
            }
            let added = word.word;
            menu.item("Add to Dictionary", move |_, cx| {
                crate::spelling::add_to_dictionary(&added, cx)
            })
        });
        let dismissed = cx.subscribe(&menu, |view, _, _: &Dismissed, cx| {
            view.spelling_menu = None;
            cx.notify();
        });
        self.spelling_menu = Some(SpellingMenu {
            menu,
            _dismissed: dismissed,
        });
        cx.notify();
    }

    /// The color misspelled words are underlined in.
    fn misspelling_color(&self) -> Hsla {
        self.theme().alert(AlertKind::Caution)
    }

    /// Moves the underlines to the text as it is now, dropping those whose
    /// words were edited, until the next check.
    pub(super) fn move_misspellings(&mut self) {
        let (Some(open), Some(old)) = (&self.open, &self.misspelled.model) else {
            return;
        };
        let new = open.editor.model();
        if Rc::ptr_eq(old, new) {
            return;
        }
        let moved = marks::moved(&self.misspelled.marks, &old.text, &new.text);
        self.misspelled.marks = Rc::new(moved);
        self.misspelled.model = Some(new.clone());
    }

    /// Moves the underlines to the text as it is now, in the theme's
    /// current color, before drawing.
    pub(super) fn refresh_misspellings(&mut self) {
        self.move_misspellings();
        let color = self.misspelling_color();
        let stale = self
            .misspelled
            .marks
            .first()
            .is_some_and(|mark| mark.style != MarkStyle::Wavy(color));
        if stale {
            let recolored = self
                .misspelled
                .marks
                .iter()
                .map(|mark| Mark {
                    style: MarkStyle::Wavy(color),
                    ..mark.clone()
                })
                .collect();
            self.misspelled.marks = Rc::new(recolored);
        }
    }

    /// The items of the spelling menu, if it's open, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn spelling_menu_items(&self, cx: &gpui::App) -> Option<Vec<String>> {
        let menu = self.spelling_menu.as_ref()?;
        Some(menu.menu.read(cx).labels())
    }

    /// The misspelled words' ranges in the text as it is now, for the
    /// smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn misspelled_ranges(&mut self) -> Vec<std::ops::Range<usize>> {
        self.move_misspellings();
        self.misspelled
            .marks
            .iter()
            .map(|mark| mark.range.clone())
            .collect()
    }
}
