//! Clicking in the document, headless: links, checkboxes and plain text
//! (`clicks.md`), and Page Down, revealing and dragging across blocks
//! (`showcase.md`).

mod common;

use gpui::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use kertas_editor::DocumentEvent;
use kertas_editor::DocumentView;
use kertas_editor::actions::{PageDown, Undo, Unreveal};

/// A point in the text of the view's `index`th block, past its first
/// character or two.
fn in_text(view: &Entity<DocumentView>, cx: &mut VisualTestContext, index: usize) -> Point<Pixels> {
    common::origin(view, cx, index) + point(px(12.), px(10.))
}

/// Clicks the text of the view's `index`th block.
fn click_text(view: &Entity<DocumentView>, cx: &mut VisualTestContext, index: usize) {
    let position = in_text(view, cx, index);
    common::click(cx, position, MouseButton::Left, Modifiers::default());
}

/// Clicking a link follows it and leaves its block rendered; clicking a
/// checkbox ticks it, as one undo step, leaving its list rendered;
/// clicking plain text reveals its block. Where a link to a Markdown file
/// leads is unit-tested in kertas-core's `links.rs`, and the `anchors`
/// smoke mode follows one into a tab.
#[gpui::test]
fn clicking_links_checkboxes_and_text(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing(&common::fixture("clicks.md"), cx);
    view.update(cx, |view, _| view.set_links_to_host(true));

    // The first block starts with a link to other.md.
    click_text(&view, cx, 0);
    assert_eq!(
        *events.borrow(),
        [DocumentEvent::LinkClicked {
            destination: "other.md".into(),
            wiki: false,
        }]
    );
    let (revealed, selection) = view.read_with(cx, |view, _| view.reveal_state());
    assert_eq!(
        (revealed, selection),
        (0, None),
        "the link's block stays rendered"
    );

    // The third block is a task list.
    click_text(&view, cx, 2);
    view.read_with(cx, |view, _| {
        assert!(
            view.text().ends_with("- [x] Task\n"),
            "clicking a checkbox ticks it"
        );
        assert!(view.is_dirty());
        assert_eq!(view.reveal_state(), (0, None), "leaving its list rendered");
    });
    cx.dispatch_action(Undo);
    cx.dispatch_action(Unreveal);
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.text())
            .ends_with("- [ ] Task\n")
    );

    // Plain text in the second block reveals it.
    let start = view.read_with(cx, |view, _| view.item_start(1)).unwrap();
    click_text(&view, cx, 1);
    let (revealed, selection) = view.read_with(cx, |view, _| view.reveal_state());
    assert_eq!(revealed, 1, "clicking plain text reveals its block");
    assert!(
        selection.is_some_and(|selection| selection.head > start),
        "{selection:?}"
    );
}

/// Page Down scrolls without moving the caret; clicking a block below a
/// revealed one keeps it in place (C7); dragging selects across blocks and
/// reveals them all.
#[gpui::test]
fn page_down_revealing_and_dragging(cx: &mut TestAppContext) {
    let (view, cx, _) = common::showing(&common::fixture("showcase.md"), cx);
    let click = |cx: &mut VisualTestContext, offset: usize| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.click(offset, false, window, cx)));
        cx.run_until_parked();
    };
    let state = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| (view.scroll_top(), view.reveal_state().1))
    };
    let position = |cx: &mut VisualTestContext, index: usize| {
        view.read_with(cx, |view, _| view.item_position(index))
    };

    click(cx, 0);
    let (scroll_before, selection_before) = state(cx);
    cx.dispatch_action(PageDown);
    cx.run_until_parked();
    let (scroll_after, selection_after) = state(cx);
    assert_ne!(scroll_after, scroll_before, "Page Down scrolls");
    assert_eq!(selection_after, selection_before, "the caret stays");

    // Well away from the top, so there's room to scroll, reveal a block,
    // then click one further down: the first collapses above it, and the
    // clicked block stays put.
    for _ in 0..2 {
        cx.dispatch_action(PageDown);
        cx.run_until_parked();
    }
    let first = state(cx).0.map_or(0, |(item, _)| item) + 1;
    let (start, _) = position(cx, first).unwrap();
    click(cx, start);
    let (start, before) = position(cx, first + 3).unwrap();
    click(cx, start);
    let (_, after) = position(cx, first + 3).unwrap();
    assert!(
        (before - after).abs() < px(1.),
        "the clicked block moved: {before:?} -> {after:?}"
    );

    // Drag from there two blocks on: all three reveal.
    let target = position(cx, first + 5).unwrap().0 + 1;
    view.update(cx, |view, cx| view.drag_to(target, cx));
    cx.run_until_parked();
    let (revealed, selection) = view.read_with(cx, |view, _| view.reveal_state());
    assert_eq!(revealed, 3, "dragging reveals the blocks it selects across");
    assert_eq!(selection.map(|selection| selection.head), Some(target));
    view.update(cx, |view, _| view.end_drag());
}
