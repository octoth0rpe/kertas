//! Spelling, on `spelling.md`, headless: underlines (#141), the menu of a
//! misspelled word (#142), and what a host can set (#143). The settings'
//! language, View → Check Spelling, Add for This Document and Add to
//! Dictionary write Kertas's settings, so the `spelling` smoke mode checks
//! those.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};
use kertas_editor::{DocumentEvent, DocumentView};

/// The words underlined as misspelled.
fn misspelled(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.update(cx, |view, _| {
        let text = view.text();
        view.misspelled_ranges()
            .into_iter()
            .map(|range| text[range].to_owned())
            .collect()
    })
}

/// `spelling.md`, once its spelling is checked.
fn spelling(cx: &mut TestAppContext) -> (Entity<DocumentView>, &mut VisualTestContext) {
    let (view, cx, _) = common::showing(&common::fixture("spelling.md"), cx);
    common::settle(cx);
    (view, cx)
}

/// Right-clicks the first word of the `index`th block, and returns the
/// items of the menu that opens.
fn right_click(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
    index: usize,
) -> Vec<String> {
    let position = common::origin(view, cx, index) + point(px(12.), px(10.));
    common::click(cx, position, MouseButton::Right, Modifiers::default());
    view.read_with(cx, |view, cx| view.spelling_menu_items(cx))
        .expect("the spelling menu opens")
}

fn set_config(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut kertas_editor::EditorConfig),
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut config = view.config().clone();
            change(&mut config);
            view.set_config(config, window, cx);
        })
    });
    common::settle(cx);
}

#[gpui::test]
fn misspelled_words_are_underlined(cx: &mut TestAppContext) {
    let (view, cx) = spelling(cx);
    assert_eq!(
        misspelled(&view, cx),
        [
            "Teh", "wrod", "lnik", "Colour", "Zorbly", "Quixlet", "Blimsy"
        ]
    );

    let at = view
        .read_with(cx, |view, _| view.text())
        .find("wrod")
        .unwrap();
    view.update(cx, |view, cx| {
        view.replace_ranges(vec![(at..at + 4, "word".into())], cx)
    });
    let right_away = misspelled(&view, cx);
    assert!(
        !right_away.contains(&"word".to_owned()) && !right_away.contains(&"wrod".to_owned()),
        "an edited word loses its underline straight away: {right_away:?}"
    );
    common::settle(cx);
    assert_eq!(
        misspelled(&view, cx),
        ["Teh", "lnik", "Colour", "Zorbly", "Quixlet", "Blimsy"],
        "and the rest stay"
    );
}

/// The configuration's language and whether spelling is checked, as the
/// settings set them.
#[gpui::test]
fn the_configuration_sets_the_language(cx: &mut TestAppContext) {
    let (view, cx) = spelling(cx);
    set_config(&view, cx, |config| {
        config.spelling_language = "en-GB".into()
    });
    assert_eq!(
        misspelled(&view, cx),
        [
            "Teh", "wrod", "lnik", "color", "Zorbly", "Quixlet", "Blimsy"
        ],
        "in British English, colour is right and color isn't"
    );
    set_config(&view, cx, |config| config.check_spelling = false);
    assert!(misspelled(&view, cx).is_empty());
    set_config(&view, cx, |config| config.check_spelling = true);
    assert_eq!(misspelled(&view, cx).len(), 7);
}

/// A misspelled word's menu: suggestions, then Ignore and Add to
/// Dictionary (Add for This Document needs a file), used with the
/// keyboard.
#[gpui::test]
fn the_menu_of_a_misspelled_word(cx: &mut TestAppContext) {
    let (view, cx) = spelling(cx);
    let items = right_click(&view, cx, 0);
    assert!(items.contains(&"The".to_owned()), "{items:?}");
    assert!(
        items.ends_with(&["Ignore".to_owned(), "Add to Dictionary".to_owned()]),
        "{items:?}"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.spelling_menu_items(cx)),
        None,
        "Esc closes it"
    );

    let items = right_click(&view, cx, 0);
    let the = items.iter().position(|item| item == "The").unwrap();
    for _ in 0..=the {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");
    common::settle(cx);
    assert!(
        view.read_with(cx, |view, _| view.text())
            .starts_with("# The spelling test"),
        "a suggestion replaces the word"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.spelling_menu_items(cx)),
        None
    );
    assert_eq!(
        misspelled(&view, cx),
        ["wrod", "lnik", "Colour", "Zorbly", "Quixlet", "Blimsy"],
        "and it's spelled right"
    );

    // Zorbly: Ignore, just above Add to Dictionary.
    right_click(&view, cx, 3);
    cx.simulate_keystrokes("end up enter");
    common::settle(cx);
    assert_eq!(
        misspelled(&view, cx),
        ["wrod", "lnik", "Colour", "Quixlet", "Blimsy"],
        "Ignore makes a word known"
    );
}

/// A host's known words and language, and its own item in the menu in
/// place of Add for This Document.
#[gpui::test]
fn what_a_host_can_set(cx: &mut TestAppContext) {
    let (view, cx) = spelling(cx);
    set_config(&view, cx, |config| {
        config.spelling_language = "en-GB".into()
    });
    view.update(cx, |view, cx| {
        view.set_known_words(vec!["lnik".into(), "Teh".into(), "wrod".into()], cx)
    });
    common::settle(cx);
    assert_eq!(
        misspelled(&view, cx),
        ["color", "Zorbly", "Quixlet", "Blimsy"],
        "a host's words are known"
    );

    view.update(cx, |view, cx| {
        view.set_spelling_language(Some("en-US".into()), cx)
    });
    common::settle(cx);
    assert_eq!(
        misspelled(&view, cx),
        ["Colour", "Zorbly", "Quixlet", "Blimsy"],
        "a host's language replaces the configuration's"
    );

    let added = Rc::new(RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let added = added.clone();
        cx.subscribe(&view, move |_, event: &DocumentEvent, _| {
            if let DocumentEvent::AddWord { word } = event {
                added.borrow_mut().push(word.clone());
            }
        })
        .detach();
    });
    view.update(cx, |view, _| {
        view.set_add_word_item(Some("Add to Project".into()))
    });
    let items = right_click(&view, cx, 2);
    assert!(
        items.ends_with(&[
            "Ignore".to_owned(),
            "Add to Project".to_owned(),
            "Add to Dictionary".to_owned(),
        ]),
        "{items:?}"
    );
    cx.simulate_keystrokes("end up enter");
    cx.run_until_parked();
    assert_eq!(*added.borrow(), ["Colour"], "it sends the word");

    view.update(cx, |view, cx| view.set_spelling_language(None, cx));
    common::settle(cx);
    assert_eq!(
        misspelled(&view, cx),
        ["color", "Zorbly", "Quixlet", "Blimsy"],
        "without it, the configuration's language is back"
    );
}
