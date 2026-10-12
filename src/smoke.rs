//! A debug-only smoke test that drives the app with key actions and typing.
//!
//! Run `KAMI_SMOKE=1 cargo run -- some/file.md`: Kami opens the file, sends
//! itself a long sequence of movement and editing steps one frame apart,
//! then prints `SMOKE OK` and quits. A panic along the way fails it. This
//! exercises the real view, list and text layout, which unit tests can't.
//! It never saves, and it doesn't touch the clipboard.

use std::time::Duration;

use gpui::{Action, App, EntityInputHandler, WindowHandle};

use crate::document_view::{
    Backspace, Delete, DeleteWordLeft, DocumentView, MoveDocumentEnd, MoveDocumentStart, MoveDown,
    MoveLeft, MoveRight, MoveUp, Newline, Redo, SelectAll, SelectDown, SelectUp, Undo,
};

const STEP: Duration = Duration::from_millis(16);

enum Step {
    Action(Box<dyn Action>),
    Type(&'static str),
}

/// Starts the smoke test if `KAMI_SMOKE` is set.
pub fn start_if_requested(window: WindowHandle<DocumentView>, cx: &mut App) {
    if std::env::var_os("KAMI_SMOKE").is_none() {
        return;
    }
    let mut steps: Vec<Step> = Vec::new();
    let steps_ref = &mut steps;
    let repeat = |steps: &mut Vec<Step>, action: &dyn Action, count: usize| {
        steps.extend((0..count).map(|_| Step::Action(action.boxed_clone())));
    };
    // Movement.
    repeat(steps_ref, &MoveDocumentStart, 1);
    repeat(steps_ref, &MoveDown, 200);
    repeat(steps_ref, &MoveUp, 200);
    repeat(steps_ref, &MoveRight, 300);
    repeat(steps_ref, &MoveLeft, 300);
    repeat(steps_ref, &MoveDocumentEnd, 1);
    repeat(steps_ref, &SelectUp, 30);
    repeat(steps_ref, &SelectDown, 30);

    // Editing at the start: a new heading and paragraph, then undo/redo.
    repeat(steps_ref, &MoveDocumentStart, 1);
    steps_ref.push(Step::Type("# Smoke"));
    repeat(steps_ref, &Newline, 2);
    steps_ref.push(Step::Type("Typed **bold** text"));
    repeat(steps_ref, &Newline, 2);
    repeat(steps_ref, &Backspace, 5);
    repeat(steps_ref, &Undo, 4);
    repeat(steps_ref, &Redo, 2);
    // Edits sprinkled through the document, including joining blocks.
    for _ in 0..40 {
        repeat(steps_ref, &MoveDown, 3);
        steps_ref.push(Step::Type("x"));
        repeat(steps_ref, &Backspace, 2);
        repeat(steps_ref, &Delete, 1);
        repeat(steps_ref, &DeleteWordLeft, 1);
        repeat(steps_ref, &Newline, 1);
    }
    for _ in 0..30 {
        repeat(steps_ref, &Undo, 1);
    }
    // Replace everything, then empty the document entirely.
    repeat(steps_ref, &SelectAll, 1);
    steps_ref.push(Step::Type("Replaced"));
    repeat(steps_ref, &SelectAll, 1);
    repeat(steps_ref, &Backspace, 1);
    steps_ref.push(Step::Type("a"));
    repeat(steps_ref, &Backspace, 3);
    repeat(steps_ref, &Undo, 3);

    cx.spawn(async move |cx| {
        let total = steps.len();
        for step in steps {
            cx.background_executor().timer(STEP).await;
            window
                .update(cx, |view, window, cx| match step {
                    Step::Action(action) => window.dispatch_action(action, cx),
                    Step::Type(text) => view.replace_text_in_range(None, text, window, cx),
                })
                .ok();
        }
        println!("SMOKE OK ({total} steps)");
        cx.update(|cx| cx.quit());
    })
    .detach();
}
