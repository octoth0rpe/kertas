//! A debug-only smoke test that drives the app with key actions and typing.
//!
//! Run `KAMI_SMOKE=1 cargo run -- some/file.md`: Kami opens the file, sends
//! itself a long sequence of movement and editing steps one frame apart,
//! then prints `SMOKE OK` and quits. A panic along the way fails it. This
//! exercises the real view, list and text layout, which unit tests can't.
//! It doesn't save unless `KAMI_SMOKE=save` (or `write`, which runs only the
//! writing steps), and it doesn't touch the clipboard.

use std::time::Duration;

use gpui::{Action, App, EntityInputHandler, WindowHandle};

use crate::document_view::{
    Backspace, CloseWindow, Delete, DeleteWordLeft, DocumentView, IndentLines, IndentTab,
    InsertTab, LineBreak, MoveBlockDown, MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown,
    MoveLeft, MoveRight, MoveUp, New, Newline, OutdentTab, Quit, Redo, Save, SelectAll,
    SelectBlockDown, SelectDown, SelectTab1, SelectTab3, SelectTab10, SelectUp, Undo, Unreveal,
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
        Ok("settings") => return smoke_settings(window, cx),
        Ok("help") => return smoke_help(window, cx),
        Ok(_) => {}
    }
    let mut steps: Vec<Step> = Vec::new();
    let steps_ref = &mut steps;
    // Movement.
    repeat(steps_ref, &MoveDocumentStart, 1);
    repeat(steps_ref, &MoveDown, 200);
    repeat(steps_ref, &MoveUp, 200);
    repeat(steps_ref, &MoveRight, 300);
    repeat(steps_ref, &MoveLeft, 300);
    repeat(steps_ref, &MoveDocumentEnd, 1);
    repeat(steps_ref, &SelectUp, 30);
    repeat(steps_ref, &SelectDown, 30);
    repeat(steps_ref, &Unreveal, 1);
    repeat(steps_ref, &MoveBlockDown, 150);
    repeat(steps_ref, &MoveBlockUp, 150);
    repeat(steps_ref, &SelectBlockDown, 5);

    // Writing at the end: paragraphs through the gap, a list and a quote.
    // `KAMI_SMOKE=write` runs only this and saves, to check the text.
    let write_only = std::env::var("KAMI_SMOKE").as_deref() == Ok("write");
    if write_only {
        steps_ref.clear();
    }
    // The first key press only places the caret.
    repeat(steps_ref, &MoveDocumentEnd, 2);
    repeat(steps_ref, &Newline, 2);
    repeat(steps_ref, &MoveUp, 3);
    repeat(steps_ref, &MoveDown, 3);
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("New paragraph"));
    repeat(steps_ref, &LineBreak, 1);
    steps_ref.push(Step::Type("second line"));
    repeat(steps_ref, &Newline, 2);
    steps_ref.push(Step::Type("- item"));
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("[ ] task"));
    repeat(steps_ref, &IndentTab, 1);
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("sub"));
    repeat(steps_ref, &OutdentTab, 1);
    repeat(steps_ref, &Newline, 2);
    steps_ref.push(Step::Type("> quote"));
    repeat(steps_ref, &LineBreak, 1);
    repeat(steps_ref, &Newline, 3);
    repeat(steps_ref, &MoveBlockUp, 4);
    repeat(steps_ref, &MoveBlockDown, 4);
    // A code block with indentation.
    repeat(steps_ref, &MoveDocumentEnd, 1);
    steps_ref.push(Step::Type("```"));
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("fn"));
    repeat(steps_ref, &IndentTab, 1);
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("    x"));
    repeat(steps_ref, &IndentLines, 1);
    repeat(steps_ref, &Newline, 1);
    steps_ref.push(Step::Type("y"));
    repeat(steps_ref, &OutdentTab, 1);
    repeat(steps_ref, &InsertTab, 1);
    repeat(steps_ref, &Undo, 1);
    if write_only {
        steps_ref.push(Step::Action(Box::new(Save)));
    } else {
        edit_steps(steps_ref);
    }

    run(window, steps, cx);
}

fn repeat(steps: &mut Vec<Step>, action: &dyn Action, count: usize) {
    steps.extend((0..count).map(|_| Step::Action(action.boxed_clone())));
}

/// Edits all over the document, undo and redo, and replacing everything.
fn edit_steps(steps_ref: &mut Vec<Step>) {
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
        steps_ref.push(Step::Type("Saved by smoke test.\n"));
        steps_ref.push(Step::Action(Box::new(Save)));
    }
}

fn run(window: WindowHandle<DocumentView>, steps: Vec<Step>, cx: &mut App) {
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
        // With automatic tabbing on, macOS adds "Show Tab Bar" to the View
        // menu.
        #[cfg(target_os = "macos")]
        let automatic_tabbing_off = !cx.update(|_| crate::native_tabs::automatic_tabbing());
        #[cfg(not(target_os = "macos"))]
        let automatic_tabbing_off = true;
        println!("automatic tabbing off: {automatic_tabbing_off}");
        // ⌘1 switches to the first tab, ⌘3 to the third, and ⌘0 (tab 10)
        // does nothing. The selected tab is read from macOS, which doesn't
        // depend on Kami being the frontmost app.
        let mut tab_switching = true;
        #[cfg(target_os = "macos")]
        for (action, expected) in [
            (Box::new(SelectTab1) as Box<dyn Action>, Some(0)),
            (Box::new(SelectTab3), Some(2)),
            (Box::new(SelectTab10), None),
        ] {
            let selected = |cx: &mut gpui::AsyncApp| {
                window
                    .update(cx, |_, window, _| {
                        crate::native_tabs::NativeWindow::of(window)
                            .and_then(|native| crate::native_tabs::selected_tab(&native))
                    })
                    .ok()
                    .flatten()
            };
            let before = selected(cx);
            // Send the key to the selected tab, as the keyboard would.
            let target = window
                .update(cx, |_, window, cx| {
                    let id = window.window_handle().window_id();
                    cx.try_global::<gpui::SystemWindowTabController>()
                        .and_then(|controller| controller.tabs(id))
                        .and_then(|tabs| tabs.get(before?))
                        .map(|tab| tab.handle)
                })
                .ok()
                .flatten();
            if let Some(target) = target {
                target
                    .update(cx, |_, window, cx| window.dispatch_action(action, cx))
                    .ok();
            }
            cx.background_executor().timer(STEP * 20).await;
            let after = selected(cx);
            let switched = after.is_some() && after == expected.or(before);
            println!(
                "tab switch to {expected:?}: {} ({before:?} -> {after:?})",
                if switched { "ok" } else { "FAILED" }
            );
            tab_switching &= switched;
        }
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
        let ok = opened == 4
            && tabs == 4
            && strip_tabs == 4
            && tab_switching
            && tabbing_calls == 3
            && remaining == 1;
        println!("SMOKE {}", if ok { "OK" } else { "FAILED" });
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// `KAMI_SMOKE=settings`, with `KAMI_CONFIG_DIR` set to an empty
/// directory: edits to the settings file apply live, problems are
/// reported, and changing a setting keeps the file's comments.
fn smoke_settings(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::app_settings::{self, AppSettings};
    use crate::theme::Theme;

    let path = std::path::PathBuf::from(std::env::var_os("KAMI_CONFIG_DIR").unwrap_or_default())
        .join("config.toml");
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = || executor.timer(Duration::from_millis(1500));
        let theme = |cx: &mut gpui::AsyncApp| window.read_with(cx, |view, _| view.theme()).ok();
        let problems =
            |cx: &mut gpui::AsyncApp| cx.update(|cx| cx.global::<AppSettings>().problems.len());
        let mut ok = true;
        let mut check = |name: &str, passed: bool| {
            println!("{name}: {}", if passed { "ok" } else { "FAILED" });
            ok &= passed;
        };

        check("no file, no problems", problems(cx) == 0);

        std::fs::write(&path, "# Mine\ntheme = \"dark\"  # at night\n").ok();
        wait().await;
        check("edit applies live", theme(cx) == Some(Theme::dark()));

        // Editors often save by renaming a new file over the old one.
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, "theme = \"light\"\n").ok();
        std::fs::rename(&temp, &path).ok();
        wait().await;
        check(
            "replaced file applies live",
            theme(cx) == Some(Theme::light()),
        );

        std::fs::write(&path, "theme = \"blue\"\nfont = 1\n").ok();
        wait().await;
        let reported = problems(cx);
        println!("problems reported: {reported}");
        check("bad file reports problems", reported == 2);

        std::fs::write(&path, "# Mine\ntheme = \"dark\"  # at night\n").ok();
        wait().await;
        let set = cx.update(|cx| app_settings::set("theme", "light", cx));
        check("setting is written", set.is_ok());
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        check(
            "comments are kept",
            text == "# Mine\ntheme = \"light\"  # at night\n",
        );
        wait().await;
        check("written setting applies", theme(cx) == Some(Theme::light()));

        // The View menu's commands change settings.
        for action in [
            Box::new(crate::menus::UseDarkTheme) as Box<dyn Action>,
            Box::new(crate::menus::UseFullWidth),
        ] {
            window
                .update(cx, |_, window, cx| window.dispatch_action(action, cx))
                .ok();
        }
        wait().await;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        check(
            "View menu writes settings",
            text == "# Mine\ntheme = \"dark\"  # at night\ntext_width = \"full\"\n",
        );
        check("View menu theme applies", theme(cx) == Some(Theme::dark()));
        window
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(crate::menus::UseStandardWidth), cx)
            })
            .ok();
        wait().await;

        // New documents open in their own window in the windows layout.
        cx.update(|cx| app_settings::set("layout", "windows", cx))
            .ok();
        window
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(New), cx)
            })
            .ok();
        wait().await;
        let windows = cx.update(|cx| cx.windows().len());
        let grouped = window
            .update(cx, |_, window, cx| {
                let id = window.window_handle().window_id();
                cx.try_global::<gpui::SystemWindowTabController>()
                    .and_then(|controller| controller.tabs(id))
                    .map_or(1, |tabs| tabs.len())
            })
            .unwrap_or(0);
        check(
            "windows layout opens a window",
            windows == 2 && grouped == 1,
        );

        println!("SMOKE {}", if ok { "OK" } else { "FAILED" });
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// `KAMI_SMOKE=help`: the Help menu opens built-in documents once each,
/// and they can't be edited.
fn smoke_help(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::builtin::Builtin;
    use crate::menus::{OpenHelp, OpenKeyboardShortcuts};

    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = || executor.timer(STEP * 20);
        let mut ok = true;
        let mut check = |name: &str, passed: bool| {
            println!("{name}: {}", if passed { "ok" } else { "FAILED" });
            ok &= passed;
        };
        let dispatch = |cx: &mut gpui::AsyncApp, action: Box<dyn Action>| {
            window
                .update(cx, |_, window, cx| window.dispatch_action(action, cx))
                .ok();
        };
        let help_window = |cx: &mut gpui::AsyncApp| {
            cx.update(|cx| {
                cx.windows()
                    .into_iter()
                    .filter_map(|window| window.downcast::<DocumentView>())
                    .find(|window| {
                        window
                            .read(cx)
                            .is_ok_and(|view| view.builtin() == Some(Builtin::Help))
                    })
            })
        };

        dispatch(cx, Box::new(OpenHelp));
        wait().await;
        let help = help_window(cx);
        check("Kami Help opens", help.is_some());
        let Some(help) = help else {
            println!("SMOKE FAILED");
            cx.update(|cx| cx.quit());
            return;
        };
        let text = |cx: &mut gpui::AsyncApp| {
            help.read_with(cx, |view, _| view.text())
                .unwrap_or_default()
        };
        let before = text(cx);
        check("it shows the help text", before == Builtin::Help.text());

        // Try to edit it every way there is.
        help.update(cx, |view, window, cx| {
            view.click(10, false, window, cx);
            for action in [
                Box::new(MoveDown) as Box<dyn Action>,
                Box::new(Newline),
                Box::new(Backspace),
                Box::new(IndentTab),
                Box::new(SelectAll),
                Box::new(Undo),
            ] {
                window.dispatch_action(action, cx);
            }
            view.replace_text_in_range(None, "typed", window, cx);
            view.replace_text_in_range(Some(0..5), "typed", window, cx);
        })
        .ok();
        wait().await;
        check("it can't be edited", text(cx) == before);
        let state = help
            .read_with(cx, |view, _| (view.has_caret(), view.is_dirty()))
            .unwrap_or((true, true));
        check("no caret, nothing to save", state == (false, false));

        dispatch(cx, Box::new(OpenHelp));
        wait().await;
        let count = cx.update(|cx| cx.windows().len());
        check("opening it again switches to it", count == 2);
        dispatch(cx, Box::new(OpenKeyboardShortcuts));
        wait().await;
        let count = cx.update(|cx| cx.windows().len());
        check("Keyboard Shortcuts opens", count == 3);

        // Closing them doesn't ask about saving.
        let windows = cx.update(|cx| cx.windows());
        for other in windows.into_iter().filter(|other| *other != *window) {
            other
                .update(cx, |_, window, cx| {
                    window.dispatch_action(Box::new(CloseWindow), cx)
                })
                .ok();
            wait().await;
        }
        let count = cx.update(|cx| cx.windows().len());
        check("they close without asking", count == 1);

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
