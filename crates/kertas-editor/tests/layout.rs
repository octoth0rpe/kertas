//! Moving around and editing documents as the app lays them out, with the
//! real macOS text system, in headless windows: what the `caret`,
//! `caret-blocks` and `1` smoke modes did with a frame between steps.

#![cfg(target_os = "macos")]

mod common;
#[path = "../../../src/smoke/steps.rs"]
mod steps;

use gpui::{Action, Entity, EntityInputHandler, VisualTestContext};
use kertas_editor::DocumentView;
use kertas_editor::actions::{
    MoveBlockDown, MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown, MoveUp,
    SelectBlockDown,
};
use steps::Step;

/// Sends `action` to the view's window, and lets it draw.
fn dispatch(cx: &mut VisualTestContext, action: Box<dyn Action>) {
    cx.update(|window, cx| window.dispatch_action(action, cx));
    cx.run_until_parked();
}

/// Moves the caret with `action` `count` times, and names the steps after
/// which it's off screen.
fn off_screen(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
    action: &dyn Action,
    count: usize,
) -> Vec<usize> {
    (0..count)
        .filter(|_| {
            dispatch(cx, action.boxed_clone());
            view.read_with(cx, |view, _| view.caret_on_screen()) != Some(true)
        })
        .collect()
}

/// The caret stays on screen moving through a document, whichever way it
/// moves, including through blocks not laid out yet (#98, #102). Test
/// windows don't run next-frame callbacks, and each step draws one frame,
/// so a caret that only reaches the screen a frame late fails.
fn caret_stays_on_screen(fixture: &str, blocks_only: bool) {
    let mut cx = common::with_real_text();
    let (view, cx, _) = common::showing(&common::fixture(fixture), &mut cx);
    let items = view.read_with(cx, |view, _| view.item_count());
    // Each move, whether it extends the selection, and how many steps:
    // twice as many by line as by block, as blank lines count.
    let moves: [(&str, Box<dyn Action>, bool, usize); 7] = [
        ("Option-Down", Box::new(MoveBlockDown), false, items),
        ("Option-Up", Box::new(MoveBlockUp), false, items),
        ("Option-Shift-Down", Box::new(SelectBlockDown), true, items),
        ("Down", Box::new(MoveDown), false, items * 2),
        ("Up", Box::new(MoveUp), false, items * 2),
        ("Cmd-Down", Box::new(MoveDocumentEnd), false, 1),
        ("Cmd-Up", Box::new(MoveDocumentStart), false, 1),
    ];
    for (name, action, extends, count) in moves {
        if blocks_only && !matches!(name, "Option-Down" | "Option-Up") {
            continue;
        }
        let off = off_screen(&view, cx, &*action, count);
        assert!(
            off.is_empty(),
            "{name} leaves the caret off screen at steps {off:?}"
        );
        if extends {
            dispatch(cx, Box::new(MoveDocumentStart));
        }
    }
}

#[test]
fn the_caret_stays_on_screen() {
    caret_stays_on_screen("showcase.md", false);
}

/// Moving by block through a document with long and short blocks.
#[test]
fn the_caret_stays_on_screen_moving_by_block() {
    caret_stays_on_screen("anchors.md", true);
}

/// The `1` smoke mode's steps: moving, writing and editing all through the
/// document, then replacing and emptying it, without a panic. (On 10k.md
/// it stays a smoke mode: there the time goes into the editor's own work,
/// so it's no faster here.)
#[test]
fn steps_through_a_document() {
    let mut cx = common::with_real_text();
    let (view, cx, _) = common::showing(&common::fixture("showcase.md"), &mut cx);
    let items = view.read_with(cx, |view, _| view.item_count());
    let mut steps = Vec::new();
    steps::movement_steps(&mut steps, items);
    steps::writing_steps(&mut steps);
    steps::edit_steps(&mut steps);
    for step in steps {
        match step {
            Step::Action(action) => dispatch(cx, action),
            Step::Type(text) => {
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.replace_text_in_range(None, text, window, cx)
                    })
                });
                cx.run_until_parked();
            }
        }
    }
    // The last steps empty the document, type "a", and undo.
    assert!(!view.read_with(cx, |view, _| view.text()).is_empty());
}

/// Down moves the caret a line within a paragraph that wraps.
#[test]
fn down_moves_a_wrapped_line() {
    let mut cx = common::with_real_text();
    let sentence = "The quick brown fox jumps over the lazy dog. ";
    let (view, cx, _) = common::showing(&sentence.repeat(20), &mut cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.click(0, false, window, cx)));
    dispatch(cx, Box::new(MoveDown));
    let caret = view.read_with(cx, |view, _| view.caret_offset()).unwrap();
    let len = sentence.len() * 20;
    assert!(caret > 20 && caret < len / 2, "caret {caret} of {len}");
}
