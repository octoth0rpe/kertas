//! A debug-only smoke test that drives the app with key actions and typing.
//!
//! Run `KAMI_SMOKE=1 cargo run -- some/file.md`: Kami opens the file, sends
//! itself a long sequence of movement and editing steps one frame apart,
//! then prints `SMOKE OK` and quits. A panic along the way fails it. This
//! exercises the real view, list and text layout, which unit tests can't.
//! It doesn't save unless `KAMI_SMOKE=save`, and it doesn't touch the
//! clipboard.

use std::time::Duration;

use gpui::{Action, App, EntityInputHandler, WindowHandle};

use crate::document_view::{
    Backspace, CloseWindow, Delete, DeleteWordLeft, DocumentView, MoveDocumentEnd,
    MoveDocumentStart, MoveDown, MoveLeft, MoveRight, MoveUp, New, Newline, Quit, Redo, Save,
    SelectAll, SelectDown, SelectUp, Undo,
};

const STEP: Duration = Duration::from_millis(16);

enum Step {
    Action(Box<dyn Action>),
    Type(&'static str),
}

/// Starts the smoke test if `KAMI_SMOKE` is set.
pub fn start_if_requested(window: WindowHandle<DocumentView>, cx: &mut App) {
    match std::env::var("KAMI_SMOKE").as_deref() {
        Err(_) => return,
        Ok("tabs") => return smoke_tabs(window, cx),
        Ok("quit") => return smoke_quit(window, cx),
        Ok(_) => {}
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

    // `KAMI_SMOKE=save` ends by saving, to check the file on disk.
    if std::env::var("KAMI_SMOKE").as_deref() == Ok("save") {
        steps.push(Step::Type("Saved by smoke test.\n"));
        steps.push(Step::Action(Box::new(Save)));
    }

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

/// `KAMI_SMOKE=tabs`: New opens windows (tabs), and they close again.
fn smoke_tabs(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        for _ in 0..3 {
            cx.background_executor().timer(STEP * 10).await;
            window
                .update(cx, |_, window, cx| {
                    window.dispatch_action(Box::new(New), cx)
                })
                .ok();
        }
        // Hold so the windows can be inspected from outside.
        cx.background_executor().timer(Duration::from_secs(2)).await;
        let opened = cx.update(|cx| cx.windows().len());
        println!("windows after 3 x New: {opened}");
        #[cfg(target_os = "macos")]
        let tabs = window
            .update(cx, |_, window, _| {
                crate::native_tabs::NativeWindow::of(window)
                    .map_or(0, |native| crate::native_tabs::tab_group_size(&native))
            })
            .unwrap_or(0);
        #[cfg(not(target_os = "macos"))]
        let tabs = opened;
        println!("tabs in the first window's group: {tabs}");
        let strip_tabs = window
            .update(cx, |_, window, cx| {
                let id = window.window_handle().window_id();
                cx.try_global::<gpui::SystemWindowTabController>()
                    .and_then(|controller| controller.tabs(id))
                    .map_or(0, |tabs| tabs.len())
            })
            .unwrap_or(0);
        println!("tabs in the tab strip: {strip_tabs}");
        #[cfg(target_os = "macos")]
        let tabbing_calls = {
            let visible = window
                .update(cx, |_, window, _| {
                    crate::native_tabs::NativeWindow::of(window)
                        .is_some_and(|native| crate::native_tabs::tab_bar_visible(&native))
                })
                .unwrap_or(false);
            println!("native tab bar visible: {visible}");
            let calls =
                crate::native_tabs::TABBING_CALLS.load(std::sync::atomic::Ordering::Relaxed);
            println!("kami tabbing calls: {calls}");
            calls
        };
        // Kami must group the tabs itself, whatever the macOS preference.
        #[cfg(not(target_os = "macos"))]
        let tabbing_calls = 3;
        let windows = cx.update(|cx| cx.windows());
        for other in windows.into_iter().filter(|other| *other != *window) {
            other
                .update(cx, |_, window, cx| {
                    window.dispatch_action(Box::new(CloseWindow), cx)
                })
                .ok();
            cx.background_executor().timer(STEP * 10).await;
        }
        let remaining = cx.update(|cx| cx.windows().len());
        println!("windows after closing them: {remaining}");
        let ok =
            opened == 4 && tabs == 4 && strip_tabs == 4 && tabbing_calls == 3 && remaining == 1;
        println!("SMOKE {}", if ok { "OK" } else { "FAILED" });
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// `KAMI_SMOKE=quit`: Quit, with an unedited document, exits the app. The
/// caller checks that the process ends.
fn smoke_quit(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(STEP * 20).await;
        println!("dispatching Quit");
        window
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(Quit), cx)
            })
            .ok();
    })
    .detach();
}
