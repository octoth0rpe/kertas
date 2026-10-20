//! The document view in GPUI's headless test windows (spike #150), with a
//! clock the test moves on rather than real pauses. GPUI's test text
//! system gives every glyph the same width; tests/layout.rs uses the real
//! macOS one.

mod common;

use gpui::TestAppContext;
use kertas_editor::{DocumentEvent, EDIT_PAUSE, Mark, MarkStyle};

use common::{Events, untitled};

fn edits(events: &Events) -> usize {
    events
        .borrow()
        .iter()
        .filter(|event| **event == DocumentEvent::Edited)
        .count()
}

#[gpui::test]
fn edited_is_sent_once_typing_pauses(cx: &mut TestAppContext) {
    let (view, cx, events) = untitled(cx);
    cx.simulate_input("# Fresh ");
    cx.executor().advance_clock(EDIT_PAUSE / 2);
    cx.simulate_input("heading");
    cx.executor().advance_clock(EDIT_PAUSE / 2);
    cx.run_until_parked();
    assert_eq!(edits(&events), 0, "not sent while typing");

    cx.executor().advance_clock(EDIT_PAUSE);
    cx.run_until_parked();
    assert_eq!(edits(&events), 1);
    view.read_with(cx, |view, _| {
        assert_eq!(view.text(), "# Fresh heading");
        assert_eq!(view.headings()[0].text, "Fresh heading");
        assert_eq!(view.word_count(), 2);
    });
}

#[gpui::test]
fn find_goes_from_match_to_match(cx: &mut TestAppContext) {
    let (view, cx, _) = untitled(cx);
    cx.simulate_input("A bold claim, **bold** and *bold*.");
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("bold");
    cx.run_until_parked();
    let found = view.read_with(cx, |view, _| (view.find_shown(), view.find_status()));
    assert!(found.0);
    assert_eq!(found.1.1, 3, "{found:?}");
    cx.simulate_keystrokes("enter");
    let first = view.read_with(cx, |view, _| view.find_status());
    cx.simulate_keystrokes("cmd-g");
    let second = view.read_with(cx, |view, _| view.find_status());
    assert_ne!(first.0, second.0, "{first:?} {second:?}");
    cx.simulate_keystrokes("escape");
    assert!(!view.read_with(cx, |view, _| view.find_shown()));
}

#[gpui::test]
fn a_right_click_on_a_mark_asks_the_host(cx: &mut TestAppContext) {
    let (view, cx, events) = untitled(cx);
    cx.simulate_input("Dr Ada went out.\n\nSecond paragraph.");
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, cx| {
        view.set_marks(
            vec![Mark {
                range: 0..2,
                style: MarkStyle::Hidden,
                id: 7,
            }],
            cx,
        )
    });
    cx.run_until_parked();
    let origin = cx
        .update(|window, cx| view.read(cx).text_origin(0, window))
        .expect("the first block is laid out");
    let position = origin + gpui::point(gpui::px(4.), gpui::px(12.));
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Right,
        position,
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Right,
        position,
        modifiers: Default::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    let menus: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            DocumentEvent::MarkMenu { id, range, .. } => Some((*id, range.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(menus, [(7, 0..2)]);
    assert_eq!(view.read_with(cx, |view, _| view.caret_offset()), None);
}

#[gpui::test]
fn misspelled_words_are_underlined(cx: &mut TestAppContext) {
    let (view, cx, _) = untitled(cx);
    cx.simulate_input("A sentance with a mispeled word.");
    cx.executor().advance_clock(EDIT_PAUSE * 2);
    cx.run_until_parked();
    let ranges = view.update(cx, |view, _| view.misspelled_ranges());
    let text = view.read_with(cx, |view, _| view.text());
    let words: Vec<_> = ranges.iter().map(|range| &text[range.clone()]).collect();
    assert_eq!(words, ["sentance", "mispeled"]);
}

#[gpui::test]
fn page_down_scrolls(cx: &mut TestAppContext) {
    let (view, cx, _) = untitled(cx);
    // Shown rather than typed: simulate_input types a character at a time.
    let text: String = (0..200).map(|i| format!("Paragraph {i}.\n\n")).collect();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_text(&text, "Paragraphs", window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.scroll_top()),
        Some((0, gpui::px(0.)))
    );
    cx.simulate_keystrokes("pagedown");
    cx.run_until_parked();
    let (top, _) = view.read_with(cx, |view, _| view.scroll_top()).unwrap();
    assert!(top > 10, "scrolled to {top}");
}
