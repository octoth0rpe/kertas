//! The find bar (#122), replacing (#123), regular expressions (#124) and
//! what a host can do with find (#125), on `find.md`, headless. Finding
//! and replacing themselves are unit-tested in kertas-core's `find.rs`.

mod common;

use gpui::{Entity, TestAppContext, VisualTestContext};
use kertas_editor::actions::Replace;
use kertas_editor::kertas_core::find::{FindOptions, OfferedOptions};
use kertas_editor::{DocumentView, EditorConfig};

/// Whether the bar is shown, and the current match and how many there are.
fn status(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> (bool, (Option<usize>, usize)) {
    view.read_with(cx, |view, _| (view.find_shown(), view.find_status()))
}

fn selection(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> Option<std::ops::Range<usize>> {
    view.read_with(cx, |view, _| {
        view.reveal_state().1.map(|selection| selection.range())
    })
}

fn type_query(view: &Entity<DocumentView>, cx: &mut VisualTestContext, query: &str) {
    view.update(cx, |view, cx| view.type_find_query(query, cx));
    cx.run_until_parked();
}

/// ⌘F, typing a query, ↩ and ⌘G from match to match with wrapping, Esc
/// leaving the match selected, and ⌘E.
#[gpui::test]
fn find_goes_from_match_to_match(cx: &mut TestAppContext) {
    let text = common::fixture("find.md");
    let (view, cx, _) = common::showing(&text, cx);
    // "bold" as it's shown: in `**bold**`, "a bold claim" and `*bold*`.
    let starts: Vec<usize> = text.match_indices("bold").map(|(at, _)| at).collect();

    cx.simulate_keystrokes("cmd-f");
    type_query(&view, cx, "one more");
    let (on_screen, (revealed, selected)) = view.read_with(cx, |view, _| {
        (
            view.row_on_screen(view.item_count() - 1),
            view.reveal_state(),
        )
    });
    assert!(
        on_screen && revealed == 0 && selected.is_none(),
        "typing scrolls to a match far below, without selecting it"
    );
    type_query(&view, cx, "bold");
    // With no caret, from the top of the window, which is at the end.
    assert_eq!(
        status(&view, cx),
        (true, (Some(2), 3)),
        "from the top of the window"
    );

    cx.simulate_keystrokes("enter");
    assert_eq!(
        status(&view, cx).1,
        (Some(0), 3),
        "↩ wraps around to the first"
    );
    assert_eq!(selection(&view, cx), Some(starts[0]..starts[0] + 4));
    cx.simulate_keystrokes("cmd-g");
    assert_eq!(status(&view, cx).1, (Some(1), 3), "⌘G goes to the next");
    assert_eq!(selection(&view, cx), Some(starts[1]..starts[1] + 4));
    cx.simulate_keystrokes("cmd-g");
    cx.run_until_parked();
    assert_eq!(
        status(&view, cx).1,
        (Some(2), 3),
        "⌘G goes to the last, far below"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.caret_on_screen()),
        Some(true),
        "and shows it"
    );
    cx.simulate_keystrokes("cmd-g");
    assert_eq!(
        status(&view, cx).1,
        (Some(0), 3),
        "⌘G wraps around to the first"
    );
    cx.simulate_keystrokes("cmd-shift-g");
    assert_eq!(
        status(&view, cx).1,
        (Some(2), 3),
        "⇧⌘G wraps around to the last"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.text()),
        text,
        "keys typed in the bar don't edit"
    );

    cx.simulate_keystrokes("escape");
    assert!(!status(&view, cx).0, "Esc closes the bar");
    assert_eq!(
        selection(&view, cx),
        Some(starts[2]..starts[2] + 4),
        "leaving the match selected"
    );

    type_query(&view, cx, "no such text");
    cx.simulate_keystrokes("cmd-g");
    assert_eq!(
        status(&view, cx).1,
        (None, 0),
        "a query with no matches finds nothing"
    );

    let claim = text.find("claim").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_range(claim..claim + 5, window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-e");
    assert_eq!(
        status(&view, cx),
        (true, (Some(0), 1)),
        "⌘E looks for the selection"
    );
}

/// Replace selects the match first and replaces it the second time. Replace
/// All is unit-tested in kertas-core.
#[gpui::test]
fn replace_selects_then_replaces(cx: &mut TestAppContext) {
    let text = common::fixture("find.md");
    let (view, cx, _) = common::showing(&text, cx);
    let starts: Vec<usize> = text.match_indices("bold").map(|(at, _)| at).collect();
    cx.update(|window, cx| view.update(cx, |view, cx| view.click(0, false, window, cx)));
    cx.dispatch_action(kertas_editor::actions::Unreveal);

    cx.simulate_keystrokes("alt-cmd-f");
    view.update(cx, |view, cx| {
        view.type_find_query("bold", cx);
        view.type_replacement("strong", cx);
    });
    cx.run_until_parked();
    // With a query, ⌥⌘F goes to the replacement, where ↩ replaces.
    cx.simulate_keystrokes("alt-cmd-f enter");
    assert_eq!(view.read_with(cx, |view, _| view.text()), text);
    assert_eq!(
        selection(&view, cx),
        Some(starts[0]..starts[0] + 4),
        "first selects the match"
    );
    cx.dispatch_action(Replace);
    cx.run_until_parked();
    let next = starts[1] + 2;
    assert_eq!(
        view.read_with(cx, |view, _| view.text()),
        text.replacen("bold", "strong", 1)
    );
    assert_eq!(
        selection(&view, cx),
        Some(next..next + 4),
        "then replaces it and goes to the next"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.find_status()),
        (Some(0), 2)
    );
}

/// An invalid pattern shows its error and finds nothing, and a valid one
/// finds the text as it's shown. Replacing with groups is unit-tested in
/// kertas-core.
#[gpui::test]
fn regular_expressions(cx: &mut TestAppContext) {
    let (view, cx, _) = common::showing(&common::fixture("find.md"), cx);
    cx.simulate_keystrokes("alt-cmd-f");
    view.update(cx, |view, cx| {
        view.set_find_regex(true, cx);
        view.type_find_query("(b)(old", cx);
    });
    cx.run_until_parked();
    let read = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| (view.find_status(), view.find_error()))
    };
    assert_eq!(read(cx), ((None, 0), Some("unclosed group".to_owned())));
    type_query(&view, cx, "(b)(old)");
    let (status, error) = read(cx);
    assert_eq!((status.1, error), (3, None));
}

/// A host can open the bar at the match after an offset, and leave out
/// options, which turns them off.
#[gpui::test]
fn what_a_host_can_do(cx: &mut TestAppContext) {
    let text = common::fixture("find.md");
    let (view, cx, _) = common::showing(&text, cx);
    let starts: Vec<usize> = text.match_indices("bold").map(|(at, _)| at).collect();
    let regex = FindOptions {
        regex: true,
        ..FindOptions::default()
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_find_with("b.ld", regex, Some(starts[1] + 1), window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(status(&view, cx), (true, (Some(2), 3)));
    assert_eq!(selection(&view, cx), Some(starts[2]..starts[2] + 4));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let config = EditorConfig {
                find_options: OfferedOptions {
                    regex: false,
                    ..OfferedOptions::default()
                },
                ..view.config().clone()
            };
            view.set_config(config, window, cx);
        })
    });
    cx.run_until_parked();
    let after = view.read_with(cx, |view, _| {
        (view.find_options().regex, view.find_status())
    });
    assert_eq!(after, (false, (None, 0)), "leaving Regex out turns it off");
}
