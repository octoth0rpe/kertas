//! A debug-only smoke test that drives the app with key actions.
//!
//! Run `KAMI_SMOKE=1 cargo run -- some/file.md`: Kami opens the file, sends
//! itself a long sequence of movement actions one frame apart, then prints
//! `SMOKE OK` and quits. A panic along the way fails it. This exercises the
//! real view, list and text layout, which unit tests can't.

use std::time::Duration;

use gpui::{Action, App, WindowHandle};

use crate::document_view::{
    DocumentView, MoveDocumentEnd, MoveDocumentStart, MoveDown, MoveLeft, MoveRight, MoveUp,
    SelectDown, SelectUp,
};

const STEP: Duration = Duration::from_millis(16);

/// Starts the smoke test if `KAMI_SMOKE` is set.
pub fn start_if_requested(window: WindowHandle<DocumentView>, cx: &mut App) {
    if std::env::var_os("KAMI_SMOKE").is_none() {
        return;
    }
    let mut steps: Vec<Box<dyn Action>> = Vec::new();
    let mut repeat = |action: &dyn Action, count: usize| {
        steps.extend((0..count).map(|_| action.boxed_clone()));
    };
    repeat(&MoveDocumentStart, 1);
    repeat(&MoveDown, 200);
    repeat(&MoveUp, 200);
    repeat(&MoveRight, 400);
    repeat(&MoveLeft, 400);
    repeat(&MoveDocumentEnd, 1);
    repeat(&SelectUp, 30);
    repeat(&SelectDown, 30);

    cx.spawn(async move |cx| {
        let total = steps.len();
        for action in steps {
            cx.background_executor().timer(STEP).await;
            window
                .update(cx, |_, window, cx| window.dispatch_action(action, cx))
                .ok();
        }
        println!("SMOKE OK ({total} actions)");
        cx.update(|cx| cx.quit());
    })
    .detach();
}
