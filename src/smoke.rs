//! Debug-only smoke tests that drive the real app, which unit tests can't:
//! the view, the list, text layout, windows, menus and macOS integration.
//!
//! `KERTAS_SMOKE=<mode> cargo run -- some/file.md` runs one mode, prints a line
//! per check, then `SMOKE OK` (or `SMOKE FAILED`) and quits. A panic fails
//! it. `scripts/smoke.sh` runs every mode. The modes:
//!
//! - `1`: a long sequence of movement and editing steps, one frame apart.
//!   It doesn't save or touch the clipboard.
//! - `save`: the same, then types a line and saves.
//! - `write`: only the writing steps (Enter, lists, quotes, code), then
//!   saves, so the text can be checked.
//! - `bench`: times 500 inserts in the middle of the document.
//! - `tabs`, `settings`, `settings-window`, `help`, `reveal`, `clicks`,
//!   `anchors`, `footnotes`, `images`, `caret`, `reading`, `scrollbar`,
//!   `scrollbar-drag`, `a11y`, `print`, `disk`, `autosave`, `quit`,
//!   `web-images`: see
//!   each function.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Action, App, AsyncApp, EntityInputHandler, WindowHandle};

use crate::document_view::{
    Backspace, CloseWindow, Delete, DeleteWordLeft, DocumentView, IndentLines, IndentTab,
    InsertTab, LineBreak, MoveBlockDown, MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown,
    MoveLeft, MoveRight, MoveUp, New, Newline, OutdentTab, Quit, Redo, Save, SelectAll,
    SelectBlockDown, SelectDown, SelectTab1, SelectTab3, SelectTab9, SelectUp, Undo, Unreveal,
};

const STEP: Duration = Duration::from_millis(16);

/// Starts the smoke test if `KERTAS_SMOKE` is set.
pub fn start_if_requested(window: WindowHandle<DocumentView>, cx: &mut App) {
    let Ok(mode) = std::env::var("KERTAS_SMOKE") else {
        return;
    };
    match mode.as_str() {
        "tabs" => smoke_tabs(window, cx),
        "quit" => smoke_quit(window, cx),
        "settings" => smoke_settings(window, cx),
        "help" => smoke_help(window, cx),
        "settings-window" => smoke_settings_window(window, cx),
        "reveal" => smoke_reveal(window, cx),
        "clicks" => smoke_clicks(window, cx),
        "anchors" => smoke_anchors(window, cx),
        "footnotes" => smoke_footnotes(window, cx),
        "images" => smoke_images(window, cx),
        "web-images" => smoke_web_images(window, cx),
        "caret" => smoke_caret(window, cx),
        "reading" => smoke_reading(window, cx),
        "scrollbar" => smoke_scrollbar(window, cx),
        "scrollbar-drag" => smoke_scrollbar_drag(window, cx),
        "a11y" => smoke_a11y(window, cx),
        "print" => smoke_print(window, cx),
        "disk" => smoke_disk(window, cx),
        "autosave" => smoke_autosave(window, cx),
        "bench" => smoke_bench(window, cx),
        _ => smoke_steps(window, &mode, cx),
    }
}

// The harness.

/// A smoke mode's checks.
#[derive(Default)]
struct Checks {
    failed: bool,
}

impl Checks {
    /// Reports one check.
    fn check(&mut self, name: &str, passed: bool) {
        println!("{name}: {}", if passed { "ok" } else { "FAILED" });
        self.failed |= !passed;
    }

    /// Prints the overall result.
    fn report(&self) {
        println!("SMOKE {}", if self.failed { "FAILED" } else { "OK" });
    }

    /// Prints the overall result and quits.
    fn finish(self, cx: &mut AsyncApp) {
        self.report();
        cx.update(|cx| cx.quit());
    }
}

/// Waits for `frames` frames' worth of time.
async fn pause(cx: &mut AsyncApp, frames: u32) {
    cx.background_executor().timer(STEP * frames).await;
}

/// Dispatches `action` in `window`, as a key press or menu command would.
fn dispatch(window: impl Into<gpui::AnyWindowHandle>, cx: &mut AsyncApp, action: Box<dyn Action>) {
    window
        .into()
        .update(cx, |_, window, cx| window.dispatch_action(action, cx))
        .ok();
}

/// Clicks in `window` at `position`, as the mouse would: it moves there,
/// then the button goes down and, a few frames later, up.
async fn mouse_click(
    window: WindowHandle<DocumentView>,
    cx: &mut AsyncApp,
    position: gpui::Point<gpui::Pixels>,
) {
    use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};

    let send = |cx: &mut AsyncApp, event: gpui::PlatformInput| {
        gpui::AnyWindowHandle::from(window)
            .update(cx, |_, window, cx| {
                window.dispatch_event(event, cx);
            })
            .ok();
    };
    send(
        cx,
        gpui::PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }),
    );
    pause(cx, 3).await;
    send(
        cx,
        gpui::PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }),
    );
    pause(cx, 3).await;
    send(
        cx,
        gpui::PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }),
    );
}

/// Presses and releases `key` (e.g. `"shift-tab"`) in `window`, as the
/// keyboard would.
async fn press_key(window: gpui::AnyWindowHandle, cx: &mut AsyncApp, key: &str) {
    let Ok(keystroke) = gpui::Keystroke::parse(key) else {
        return;
    };
    let up = keystroke.clone();
    window
        .update(cx, |_, window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        })
        .ok();
    pause(cx, 2).await;
    window
        .update(cx, |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke: up }),
                cx,
            );
        })
        .ok();
    pause(cx, 3).await;
}

fn window_count(cx: &mut AsyncApp) -> usize {
    cx.update(|cx| cx.windows().len())
}

/// How many tabs are in `window`'s tab group, per GPUI's tab controller.
fn tab_count(window: WindowHandle<DocumentView>, cx: &mut AsyncApp) -> usize {
    window
        .update(cx, |_, window, cx| {
            let id = window.window_handle().window_id();
            cx.try_global::<gpui::SystemWindowTabController>()
                .and_then(|controller| controller.tabs(id))
                .map_or(1, |tabs| tabs.len())
        })
        .unwrap_or(0)
}

// Step-by-step editing: `1`, `save` and `write`.

enum Step {
    Action(Box<dyn Action>),
    Type(&'static str),
}

fn repeat(steps: &mut Vec<Step>, action: &dyn Action, count: usize) {
    steps.extend((0..count).map(|_| Step::Action(action.boxed_clone())));
}

fn smoke_steps(window: WindowHandle<DocumentView>, mode: &str, cx: &mut App) {
    let mut steps = Vec::new();
    if mode != "write" {
        movement_steps(&mut steps);
    }
    writing_steps(&mut steps);
    if mode == "write" {
        steps.push(Step::Action(Box::new(Save)));
    } else {
        edit_steps(&mut steps);
    }
    if mode == "save" {
        steps.push(Step::Type("Saved by smoke test.\n"));
        steps.push(Step::Action(Box::new(Save)));
    }

    cx.spawn(async move |cx| {
        let total = steps.len();
        for step in steps {
            pause(cx, 1).await;
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

/// Moving around the whole document, by character, line and block.
fn movement_steps(steps: &mut Vec<Step>) {
    // With no caret, ↑ and ↓ only scroll; → starts editing (#99).
    repeat(steps, &MoveRight, 1);
    repeat(steps, &MoveDocumentStart, 1);
    repeat(steps, &MoveDown, 200);
    repeat(steps, &MoveUp, 200);
    repeat(steps, &MoveRight, 300);
    repeat(steps, &MoveLeft, 300);
    repeat(steps, &MoveDocumentEnd, 1);
    repeat(steps, &SelectUp, 30);
    repeat(steps, &SelectDown, 30);
    repeat(steps, &Unreveal, 1);
    repeat(steps, &MoveBlockDown, 150);
    repeat(steps, &MoveBlockUp, 150);
    repeat(steps, &SelectBlockDown, 5);
}

/// Writing at the end: paragraphs through the gap, a list, a quote and a
/// code block with indentation.
fn writing_steps(steps: &mut Vec<Step>) {
    // With no caret, ⌘↓ only scrolls; → starts editing (#99).
    repeat(steps, &MoveRight, 1);
    repeat(steps, &MoveDocumentEnd, 1);
    repeat(steps, &Newline, 2);
    repeat(steps, &MoveUp, 3);
    repeat(steps, &MoveDown, 3);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("New paragraph"));
    repeat(steps, &LineBreak, 1);
    steps.push(Step::Type("second line"));
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("- item"));
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("[ ] task"));
    repeat(steps, &IndentTab, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("sub"));
    repeat(steps, &OutdentTab, 1);
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("> quote"));
    repeat(steps, &LineBreak, 1);
    repeat(steps, &Newline, 3);
    repeat(steps, &MoveBlockUp, 4);
    repeat(steps, &MoveBlockDown, 4);
    repeat(steps, &MoveDocumentEnd, 1);
    steps.push(Step::Type("```"));
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("fn"));
    repeat(steps, &IndentTab, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("    x"));
    repeat(steps, &IndentLines, 1);
    repeat(steps, &Newline, 1);
    steps.push(Step::Type("y"));
    repeat(steps, &OutdentTab, 1);
    repeat(steps, &InsertTab, 1);
    repeat(steps, &Undo, 1);
}

/// Edits all over the document, undo and redo, and replacing everything.
fn edit_steps(steps: &mut Vec<Step>) {
    // Editing at the start: a new heading and paragraph, then undo/redo.
    repeat(steps, &MoveDocumentStart, 1);
    steps.push(Step::Type("# Smoke"));
    repeat(steps, &Newline, 2);
    steps.push(Step::Type("Typed **bold** text"));
    repeat(steps, &Newline, 2);
    repeat(steps, &Backspace, 5);
    repeat(steps, &Undo, 4);
    repeat(steps, &Redo, 2);
    // Edits sprinkled through the document, including joining blocks.
    for _ in 0..40 {
        repeat(steps, &MoveDown, 3);
        steps.push(Step::Type("x"));
        repeat(steps, &Backspace, 2);
        repeat(steps, &Delete, 1);
        repeat(steps, &DeleteWordLeft, 1);
        repeat(steps, &Newline, 1);
    }
    repeat(steps, &Undo, 30);
    // Replace everything, then empty the document entirely.
    repeat(steps, &SelectAll, 1);
    steps.push(Step::Type("Replaced"));
    repeat(steps, &SelectAll, 1);
    repeat(steps, &Backspace, 1);
    steps.push(Step::Type("a"));
    repeat(steps, &Backspace, 3);
    repeat(steps, &Undo, 3);
}

// `bench`: typing speed.

/// Inserts timed by `bench`.
const BENCH_INSERTS: usize = 500;

/// The slowest acceptable 95th percentiles in a debug build, in
/// milliseconds: the time to handle one insert, and the time until the
/// frame showing it has been drawn (which includes waiting for the
/// display). Typical on an M2 MacBook Air (2026-10-13), debug, 10k.md:
/// about 14 ms and 30 ms. Release builds handle inserts about ten times
/// faster.
const BENCH_MAX_INSERT_MS: f64 = 60.;
const BENCH_MAX_FRAME_MS: f64 = 120.;

/// `KERTAS_SMOKE=bench`: types `BENCH_INSERTS` characters in the middle of
/// the document, one per frame, and reports the median and 95th percentile
/// time per insert and until its frame is drawn.
fn smoke_bench(window: WindowHandle<DocumentView>, cx: &mut App) {
    // macOS slows drawing for windows that aren't in front.
    cx.activate(true);
    cx.spawn(async move |cx| {
        pause(cx, 10).await;
        // Place the caret at the start of a block in the middle.
        let middle = window
            .read_with(cx, |view, _| {
                let count = view.item_count();
                view.item_start(count / 2)
            })
            .ok()
            .flatten()
            .unwrap_or(0);
        window
            .update(cx, |view, window, cx| view.click(middle, false, window, cx))
            .ok();
        pause(cx, 10).await;

        let mut insert_ms = Vec::with_capacity(BENCH_INSERTS);
        let mut frame_ms = Vec::with_capacity(BENCH_INSERTS);
        for index in 0..BENCH_INSERTS {
            let drawn: Rc<Cell<Option<Instant>>> = Rc::default();
            let started = Instant::now();
            let handled = window
                .update(cx, |view, window, cx| {
                    let text = if index % 40 == 39 { " " } else { "x" };
                    view.replace_text_in_range(None, text, window, cx);
                    let handled = started.elapsed();
                    // A next-frame callback runs before that frame is drawn,
                    // so wait for the one after: the insert's frame has
                    // then been drawn. That adds up to one display refresh.
                    let drawn = drawn.clone();
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |_, _| drawn.set(Some(Instant::now())));
                        // GPUI only draws when something changed, so ask
                        // for that next frame.
                        window.refresh();
                    });
                    handled
                })
                .ok();
            // Wait for the frame, up to a second.
            for _ in 0..1000 {
                if drawn.get().is_some() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(1))
                    .await;
            }
            if let (Some(handled), Some(drawn)) = (handled, drawn.get()) {
                insert_ms.push(handled.as_secs_f64() * 1000.);
                frame_ms.push((drawn - started).as_secs_f64() * 1000.);
            }
        }

        let mut checks = Checks::default();
        // Allow a few missed frames, e.g. if another window covered Kertas.
        checks.check(
            &format!(
                "inserts drew frames ({} of {BENCH_INSERTS})",
                frame_ms.len()
            ),
            frame_ms.len() >= BENCH_INSERTS * 95 / 100,
        );
        let (insert_p50, insert_p95) = percentiles(&mut insert_ms);
        let (frame_p50, frame_p95) = percentiles(&mut frame_ms);
        println!("insert: p50 {insert_p50:.1} ms, p95 {insert_p95:.1} ms");
        println!("insert to frame: p50 {frame_p50:.1} ms, p95 {frame_p95:.1} ms");
        checks.check(
            &format!("insert p95 under {BENCH_MAX_INSERT_MS} ms"),
            insert_p95 < BENCH_MAX_INSERT_MS,
        );
        checks.check(
            &format!("insert to frame p95 under {BENCH_MAX_FRAME_MS} ms"),
            frame_p95 < BENCH_MAX_FRAME_MS,
        );
        checks.finish(cx);
    })
    .detach();
}

/// The median and 95th percentile of `values`.
fn percentiles(values: &mut [f64]) -> (f64, f64) {
    if values.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    values.sort_by(f64::total_cmp);
    let at = |fraction: f64| values[((values.len() - 1) as f64 * fraction).round() as usize];
    (at(0.5), at(0.95))
}

// The other modes.

/// `KERTAS_SMOKE=tabs`: New opens windows as tabs, ⌘-numbers switch tabs,
/// the tabs close again, and New replaces the startup message.
fn smoke_tabs(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        for _ in 0..3 {
            pause(cx, 10).await;
            dispatch(window, cx, Box::new(New));
        }
        // Hold so the windows can be inspected from outside.
        cx.background_executor().timer(Duration::from_secs(2)).await;
        checks.check("3 x New opens 4 windows", window_count(cx) == 4);
        #[cfg(target_os = "macos")]
        {
            let native_tabs = window
                .update(cx, |_, window, _| {
                    crate::native_tabs::ns_window(window)
                        .map_or(0, |native| crate::native_tabs::tab_group_size(&native))
                })
                .unwrap_or(0);
            checks.check("they're tabs in one macOS tab group", native_tabs == 4);
        }
        checks.check("the tab strip shows them", tab_count(window, cx) == 4);
        // With automatic tabbing on, macOS adds "Show Tab Bar" to the View
        // menu.
        #[cfg(target_os = "macos")]
        checks.check(
            "automatic tabbing is off",
            !cx.update(|_| crate::native_tabs::automatic_tabbing()),
        );

        // ⌘1 switches to the first tab, ⌘3 to the third, and ⌘9 does
        // nothing with four tabs. The selected tab is read from macOS,
        // which doesn't depend on Kertas being the frontmost app.
        #[cfg(target_os = "macos")]
        for (action, expected) in [
            (Box::new(SelectTab1) as Box<dyn Action>, Some(0)),
            (Box::new(SelectTab3), Some(2)),
            (Box::new(SelectTab9), None),
        ] {
            let selected = |cx: &mut AsyncApp| {
                window
                    .update(cx, |_, window, _| {
                        crate::native_tabs::ns_window(window)
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
                dispatch(target, cx, action);
            }
            pause(cx, 20).await;
            let after = selected(cx);
            checks.check(
                &format!("tab switch to {expected:?} ({before:?} -> {after:?})"),
                after.is_some() && after == expected.or(before),
            );
        }
        // Kertas must group the tabs itself, whatever the macOS preference.
        #[cfg(target_os = "macos")]
        checks.check(
            "Kertas grouped each new window",
            crate::native_tabs::TABBING_CALLS.load(std::sync::atomic::Ordering::Relaxed) == 3,
        );

        let windows = cx.update(|cx| cx.windows());
        for other in windows.into_iter().filter(|other| *other != *window) {
            dispatch(other, cx, Box::new(CloseWindow));
            pause(cx, 10).await;
        }
        checks.check("closing them leaves 1 window", window_count(cx) == 1);

        // New in a tab showing the startup message starts the document
        // there instead of adding a tab.
        let blank = cx.update(|cx| crate::windows::open_window(cx, |_, _, _| {}));
        pause(cx, 20).await;
        if let Some(blank) = blank {
            dispatch(blank, cx, Box::new(New));
        }
        pause(cx, 20).await;
        checks.check(
            "New replaces the startup message",
            window_count(cx) == 2
                && blank.is_some_and(|blank| {
                    blank
                        .read_with(cx, |view, _| !view.is_blank())
                        .unwrap_or(false)
                }),
        );
        if let Some(blank) = blank {
            dispatch(blank, cx, Box::new(CloseWindow));
        }
        pause(cx, 10).await;

        // Files with the same name show their folders (#95).
        let folder = window
            .read_with(cx, |view, _| {
                view.document_path()
                    .and_then(Path::parent)
                    .map(Path::to_owned)
            })
            .ok()
            .flatten();
        if let Some(folder) = folder {
            for name in ["docs", "src"] {
                let path = folder.join(name).join("README.md");
                std::fs::create_dir_all(path.parent().unwrap()).ok();
                std::fs::write(&path, format!("# {name}\n")).ok();
                cx.update(|cx| crate::windows::open_path(path, cx));
                pause(cx, 20).await;
            }
        }
        let titles = |cx: &mut AsyncApp| {
            window
                .update(cx, |_, window, cx| {
                    let id = window.window_handle().window_id();
                    cx.try_global::<gpui::SystemWindowTabController>()
                        .and_then(|controller| controller.tabs(id))
                        .map(|tabs| {
                            tabs.iter()
                                .map(|tab| tab.title.to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        };
        let both = titles(cx);
        checks.check(
            &format!("same names show their folders ({both:?})"),
            both.contains(&"README.md — docs".to_owned())
                && both.contains(&"README.md — src".to_owned()),
        );
        let docs = cx.update(|cx| {
            cx.windows().into_iter().find(|other| {
                other
                    .downcast::<DocumentView>()
                    .and_then(|other| other.read(cx).ok())
                    .and_then(|view| view.document_path())
                    .is_some_and(|path| path.ends_with("docs/README.md"))
            })
        });
        if let Some(docs) = docs {
            dispatch(docs, cx, Box::new(CloseWindow));
        }
        pause(cx, 20).await;
        let one = titles(cx);
        checks.check(
            &format!("closing one drops the other's folder ({one:?})"),
            one.contains(&"README.md".to_owned()) && !one.iter().any(|title| title.contains('—')),
        );
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=settings`, with `KERTAS_CONFIG_DIR` set to an empty
/// directory: edits to the settings file apply live, problems are
/// reported, changing a setting keeps the file's comments, the View menu
/// and zoom work, and the layout setting waits for a restart.
fn smoke_settings(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::app_settings::{self, AppSettings};
    use crate::document_view::{ResetZoom, ZoomIn, ZoomOut};
    use crate::theme::Theme;

    let path = std::path::PathBuf::from(std::env::var_os("KERTAS_CONFIG_DIR").unwrap_or_default())
        .join("config.toml");
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        // Settings apply when macOS reports the file changed, which can
        // take a while on a busy machine, so checks wait for them to.
        const APPLY: Duration = Duration::from_secs(5);
        // A pause for things that shouldn't happen.
        let settle = async |cx: &mut AsyncApp| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await
        };
        let theme = |cx: &mut AsyncApp| window.read_with(cx, |view, _| view.theme()).ok();
        let problems =
            |cx: &mut AsyncApp| cx.update(|cx| cx.global::<AppSettings>().problems.len());

        checks.check("no file, no problems", problems(cx) == 0);

        // From before themes, when `theme` was the appearance.
        std::fs::write(&path, "# Mine\ntheme = \"dark\"  # at night\n").ok();
        checks.check(
            "edit applies live (old appearance setting)",
            wait_for(cx, APPLY, |cx| theme(cx) == Some(Theme::dark())).await,
        );

        // Editors often save by renaming a new file over the old one.
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, "appearance = \"light\"\n").ok();
        std::fs::rename(&temp, &path).ok();
        checks.check(
            "replaced file applies live",
            wait_for(cx, APPLY, |cx| theme(cx) == Some(Theme::light())).await,
        );

        std::fs::write(&path, "theme = \"blue\"\nfont = 1\n").ok();
        let reported = wait_for(cx, APPLY, |cx| problems(cx) == 2).await;
        checks.check("bad file reports 2 problems", reported);
        // With no block being edited, Tab reaches the bar's dismiss button
        // and Space presses it.
        let shown = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.settings_problems_shown())
                .unwrap_or(false)
        };
        let shown_before = shown(cx);
        press_key(window.into(), cx, "tab").await;
        press_key(window.into(), cx, "space").await;
        pause(cx, 5).await;
        checks.check(
            "Tab and Space dismiss the problems bar",
            reported && shown_before && !shown(cx),
        );

        std::fs::write(&path, "# Mine\ntheme = \"dark\"  # at night\n").ok();
        wait_for(cx, APPLY, |cx| {
            problems(cx) == 0 && theme(cx) == Some(Theme::dark())
        })
        .await;
        let set = cx.update(|cx| app_settings::set("appearance", "light", cx));
        checks.check("setting is written", set.is_ok());
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        checks.check(
            "old setting is renamed, comments are kept",
            text == "# Mine\nappearance = \"light\"  # at night\n",
        );
        checks.check(
            "written setting applies",
            wait_for(cx, APPLY, |cx| theme(cx) == Some(Theme::light())).await,
        );

        // The View menu's commands change settings.
        dispatch(window, cx, Box::new(crate::menus::UseDarkAppearance));
        dispatch(window, cx, Box::new(crate::menus::UseFullWidth));
        let dark = wait_for(cx, APPLY, |cx| theme(cx) == Some(Theme::dark())).await;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        checks.check(
            "View menu writes settings",
            text == "# Mine\nappearance = \"dark\"  # at night\ntext_width = \"full\"\n",
        );
        checks.check("View menu appearance applies", dark);
        let use_theme = |name: &str| {
            Box::new(crate::menus::UseTheme {
                name: name.to_owned().into(),
            })
        };
        dispatch(window, cx, use_theme("Solarized"));
        let solarized = wait_for(cx, APPLY, |cx| {
            theme(cx).is_some_and(|theme| theme != Theme::dark())
        })
        .await;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        checks.check(
            "View menu theme applies",
            solarized && text.contains("theme = \"Solarized\"\n"),
        );
        dispatch(window, cx, use_theme("Kertas"));
        let kertas = wait_for(cx, APPLY, |cx| theme(cx) == Some(Theme::dark())).await;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        checks.check(
            "choosing Kertas removes the setting",
            kertas && !text.contains("theme ="),
        );

        // The user's themes, in themes/ next to the settings file, are
        // listed, and adding, editing and removing them applies live.
        let folder = path.with_file_name("themes");
        let mine = folder.join("Mine.toml");
        let kertas_file = kertas_core::themes::KERTAS_FILE;
        let dark_background = format!(
            "background = \"#{:06x}\"",
            kertas_core::themes::kertas().dark.background.0 >> 8
        );
        let with_background = |name: &str, color: &str| {
            kertas_file
                .replacen("name = \"Kertas\"", &format!("name = \"{name}\""), 1)
                .replacen(&dark_background, &format!("background = \"{color}\""), 1)
        };
        let background = |cx: &mut AsyncApp| theme(cx).map(|theme| theme.background);
        let color = |value: u32| Some(gpui::Hsla::from(gpui::rgba(value)));
        let listed = |cx: &mut AsyncApp, name: &'static str| {
            cx.update(|cx| {
                app_settings::themes(cx)
                    .iter()
                    .any(|theme| theme.name == name)
            })
        };
        std::fs::create_dir_all(&folder).ok();
        std::fs::write(&mine, with_background("Mine", "#102030")).ok();
        checks.check(
            "user theme is listed",
            wait_for(cx, APPLY, |cx| listed(cx, "Mine")).await,
        );
        dispatch(window, cx, use_theme("Mine"));
        checks.check(
            "user theme applies",
            wait_for(cx, APPLY, |cx| background(cx) == color(0x102030ff)).await,
        );
        std::fs::write(&mine, with_background("Mine", "#203040")).ok();
        checks.check(
            "editing a user theme applies live",
            wait_for(cx, APPLY, |cx| background(cx) == color(0x203040ff)).await,
        );
        std::fs::write(
            folder.join("Solarized.toml"),
            with_background("solarized", "#304050"),
        )
        .ok();
        wait_for(cx, APPLY, |cx| {
            cx.update(|cx| {
                app_settings::themes(cx)
                    .iter()
                    .any(|theme| theme.dark.background == kertas_core::themes::Color(0x304050ff))
            })
        })
        .await;
        dispatch(window, cx, use_theme("Solarized"));
        checks.check(
            "a user theme replaces the built-in one of its name",
            wait_for(cx, APPLY, |cx| background(cx) == color(0x304050ff)).await,
        );
        dispatch(window, cx, use_theme("Mine"));
        std::fs::write(folder.join("Bad.toml"), "[light]\n").ok();
        checks.check(
            "problems in theme files are reported, naming the file",
            wait_for(cx, APPLY, |cx| {
                cx.update(|cx| {
                    cx.global::<AppSettings>()
                        .theme_problems
                        .iter()
                        .any(|problem| problem.file == "Bad.toml")
                })
            })
            .await,
        );
        std::fs::remove_file(&mine).ok();
        checks.check(
            "removing the chosen theme falls back to Kertas's",
            wait_for(cx, APPLY, |cx| {
                !listed(cx, "Mine") && theme(cx) == Some(Theme::dark()) && problems(cx) == 1
            })
            .await,
        );
        std::fs::remove_dir_all(&folder).ok();
        cx.update(|cx| app_settings::remove_or_report("theme", cx));
        wait_for(cx, APPLY, |cx| {
            problems(cx) == 0
                && cx.update(|cx| cx.global::<AppSettings>().theme_problems.is_empty())
        })
        .await;
        dispatch(window, cx, Box::new(crate::menus::UseStandardWidth));
        settle(cx).await;

        // Zoom steps through sizes, stops at the limits, and Reset Zoom
        // returns to the font_size setting, which also resets it. Each step
        // waits for a frame: zoom commands are only available when the last
        // frame allowed them.
        async fn zoom(
            window: WindowHandle<DocumentView>,
            cx: &mut AsyncApp,
            action: Box<dyn Action>,
        ) -> u32 {
            dispatch(window, cx, action);
            pause(cx, 3).await;
            cx.update(|cx| crate::zoom::size(cx))
        }
        checks.check("zoom in", zoom(window, cx, Box::new(ZoomIn)).await == 17);
        checks.check(
            "reset zoom",
            zoom(window, cx, Box::new(ResetZoom)).await == 16,
        );
        for _ in 0..20 {
            zoom(window, cx, Box::new(ZoomOut)).await;
        }
        let at_limit = window
            .update(cx, |_, window, cx| {
                (
                    crate::zoom::size(cx),
                    window.is_action_available(&ZoomOut, cx),
                )
            })
            .ok();
        checks.check(
            "zoom out stops at 10, disabled",
            at_limit == Some((10, false)),
        );
        cx.update(|cx| app_settings::set("font_size", 20, cx)).ok();
        checks.check(
            "font_size setting sets the size",
            wait_for(cx, APPLY, |cx| cx.update(|cx| crate::zoom::size(cx)) == 20).await,
        );

        cx.update(|cx| app_settings::set("editing_hint", false, cx))
            .ok();
        checks.check(
            "editing_hint setting applies",
            wait_for(cx, APPLY, |cx| {
                !cx.update(|cx| app_settings::get(cx).editing_hint)
            })
            .await,
        );

        cx.update(|cx| app_settings::set("tab_width", 8, cx)).ok();
        checks.check(
            "tab_width setting applies",
            wait_for(cx, APPLY, |cx| {
                cx.update(|cx| app_settings::get(cx).tab_width) == 8
            })
            .await,
        );

        cx.update(|cx| app_settings::set("list_indent", "wide", cx))
            .ok();
        checks.check(
            "list_indent setting applies to the document",
            wait_for(cx, APPLY, |cx| {
                window
                    .read_with(cx, |view, _| view.list_indent())
                    .is_ok_and(|indent| indent == kertas_core::settings::ListIndent::Wide)
            })
            .await,
        );

        // Changing the layout waits until Kertas restarts: New still opens a
        // tab.
        cx.update(|cx| app_settings::set("layout", "windows", cx))
            .ok();
        dispatch(window, cx, Box::new(New));
        settle(cx).await;
        checks.check(
            "layout change waits for a restart",
            window_count(cx) == 2 && tab_count(window, cx) == 2,
        );
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=help`: the Help and Kertas menus open built-in documents once
/// each, they can't be edited, and they close without asking.
fn smoke_help(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::builtin::Builtin;
    use crate::menus::{
        OpenAbout, OpenHelp, OpenKeyboardShortcuts, OpenLicenses, OpenMarkdownSyntax,
        OpenReleaseNotes, OpenThemes,
    };

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        let help_window = |cx: &mut AsyncApp| {
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

        dispatch(window, cx, Box::new(OpenHelp));
        pause(cx, 20).await;
        let help = help_window(cx);
        checks.check("Kertas Help opens", help.is_some());
        let Some(help) = help else {
            return checks.finish(cx);
        };
        let text = |cx: &mut AsyncApp| {
            help.read_with(cx, |view, _| view.text())
                .unwrap_or_default()
        };
        let before = text(cx);
        checks.check("it shows the help text", before == Builtin::Help.text());

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
        pause(cx, 20).await;
        checks.check("it can't be edited", text(cx) == before);
        let state = help
            .read_with(cx, |view, _| (view.has_caret(), view.is_dirty()))
            .unwrap_or((true, true));
        checks.check("no caret, nothing to save", state == (false, false));

        dispatch(window, cx, Box::new(OpenHelp));
        pause(cx, 20).await;
        checks.check("opening it again switches to it", window_count(cx) == 2);
        dispatch(window, cx, Box::new(OpenMarkdownSyntax));
        pause(cx, 20).await;
        checks.check("Markdown Syntax opens", window_count(cx) == 3);
        dispatch(window, cx, Box::new(OpenKeyboardShortcuts));
        pause(cx, 20).await;
        checks.check("Keyboard Shortcuts opens", window_count(cx) == 4);
        dispatch(window, cx, Box::new(OpenThemes));
        pause(cx, 20).await;
        checks.check("Themes opens", window_count(cx) == 5);
        dispatch(window, cx, Box::new(OpenReleaseNotes));
        pause(cx, 20).await;
        checks.check("Release Notes opens", window_count(cx) == 6);
        dispatch(window, cx, Box::new(OpenAbout));
        pause(cx, 20).await;
        checks.check("About Kertas opens", window_count(cx) == 7);
        dispatch(window, cx, Box::new(OpenLicenses));
        pause(cx, 20).await;
        checks.check("Licenses opens", window_count(cx) == 8);

        let windows = cx.update(|cx| cx.windows());
        for other in windows.into_iter().filter(|other| *other != *window) {
            dispatch(other, cx, Box::new(CloseWindow));
            pause(cx, 20).await;
        }
        checks.check("they close without asking", window_count(cx) == 1);
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=settings-window`: ⌘, opens one settings sheet; New from it
/// still opens a tab; Esc closes it; and Quit with it open quits. The
/// caller checks that the process ends.
fn smoke_settings_window(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::menus::OpenSettings;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        for _ in 0..2 {
            dispatch(window, cx, Box::new(OpenSettings));
            pause(cx, 20).await;
        }
        checks.check("⌘, opens one settings window", window_count(cx) == 2);
        // The sheet fits itself to its rows: they end 20 px above the Done
        // button, with autosave off or on (its note adds a line).
        for autosave in ["off", "on"] {
            if autosave == "on" {
                cx.update(|cx| crate::app_settings::set("autosave", 5, cx))
                    .ok();
                pause(cx, 10).await;
            }
            let spare = cx.update(|cx| {
                cx.try_global::<crate::settings_window::RowsFit>()
                    .map(|fit| fit.footer_top - fit.rows_bottom)
            });
            checks.check(
                &format!(
                    "every row fits in the settings window, autosave {autosave} \
                     ({spare:?} below them)"
                ),
                spare.is_some_and(|spare| (spare - gpui::px(20.)).abs() <= gpui::px(1.)),
            );
        }

        let settings = cx.update(|cx| {
            cx.windows()
                .into_iter()
                .find(|other| other.downcast::<DocumentView>().is_none())
        });
        if let Some(settings) = settings {
            dispatch(settings, cx, Box::new(New));
        }
        pause(cx, 20).await;
        checks.check(
            "New from Settings opens a tab",
            window_count(cx) == 3 && tab_count(window, cx) == 2,
        );

        // From the keyboard: Tab reaches the first control (Appearance's
        // Light), Space chooses it, and Tab moves on to Dark.
        let theme = |cx: &mut AsyncApp| cx.update(|cx| crate::app_settings::get(cx).appearance);
        if let Some(settings) = settings {
            settings
                .update(cx, |_, window, _| window.activate_window())
                .ok();
            pause(cx, 10).await;
            press_key(settings, cx, "tab").await;
            press_key(settings, cx, "space").await;
            pause(cx, 10).await;
            let light = theme(cx);
            press_key(settings, cx, "tab").await;
            press_key(settings, cx, "space").await;
            pause(cx, 10).await;
            let dark = theme(cx);
            press_key(settings, cx, "shift-tab").await;
            press_key(settings, cx, "space").await;
            pause(cx, 10).await;
            let back = theme(cx);
            checks.check(
                &format!("Tab, ⇧Tab and Space choose settings ({light:?}, {dark:?}, {back:?})"),
                light == kertas_core::settings::AppearanceSetting::Light
                    && dark == kertas_core::settings::AppearanceSetting::Dark
                    && back == kertas_core::settings::AppearanceSetting::Light,
            );
            settings
                .update(cx, |_, window, cx| {
                    window.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), cx);
                })
                .ok();
        }
        pause(cx, 20).await;
        checks.check("Esc closes Settings", window_count(cx) == 2);

        // Quit with Settings open still quits.
        dispatch(window, cx, Box::new(OpenSettings));
        pause(cx, 20).await;
        checks.report();
        println!("dispatching Quit");
        dispatch(window, cx, Box::new(Quit));
    })
    .detach();
}

/// `KERTAS_SMOKE=reveal`: Page Down scrolls without moving the caret,
/// clicking a block below a revealed one keeps it in place (C7), and
/// dragging selects across blocks and reveals them all.
fn smoke_reveal(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::document_view::PageDown;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let position = |cx: &mut AsyncApp, index: usize| {
            window
                .read_with(cx, |view, _| view.item_position(index))
                .ok()
                .flatten()
        };
        let click = |cx: &mut AsyncApp, offset: usize| {
            window
                .update(cx, |view, window, cx| view.click(offset, false, window, cx))
                .ok();
        };
        let state = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| (view.scroll_top(), view.reveal_state().1))
                .unwrap_or_default()
        };

        click(cx, 0);
        pause(cx, 10).await;
        let (scroll_before, selection_before) = state(cx);
        dispatch(window, cx, Box::new(PageDown));
        pause(cx, 10).await;
        let (scroll_after, selection_after) = state(cx);
        checks.check(
            &format!("Page Down scrolls, caret stays ({scroll_before:?} -> {scroll_after:?})"),
            scroll_after != scroll_before && selection_after == selection_before,
        );

        // Now well away from the top, so there's room to scroll, reveal a
        // block, then click one further down: the first collapses above
        // it, and the clicked block should stay put.
        for _ in 0..2 {
            dispatch(window, cx, Box::new(PageDown));
            pause(cx, 10).await;
        }
        let (scroll_now, _) = state(cx);
        let first = scroll_now.map_or(0, |(item, _)| item) + 1;
        let (start, _) = position(cx, first).unwrap_or_default();
        click(cx, start);
        pause(cx, 10).await;
        let before = position(cx, first + 3);
        if let Some((start, _)) = before {
            click(cx, start);
        }
        pause(cx, 10).await;
        let after = position(cx, first + 3);
        checks.check(
            &format!("clicked block stays in place ({before:?} -> {after:?})"),
            matches!((before, after), (Some((_, a)), Some((_, b))) if (a - b).abs() < gpui::px(1.)),
        );

        // Drag from there two blocks on: all three reveal.
        let target = position(cx, first + 5).map(|(start, _)| start + 1);
        if let Some(target) = target {
            window
                .update(cx, |view, _, cx| view.drag_to(target, cx))
                .ok();
        }
        pause(cx, 10).await;
        let (revealed, selection) = window
            .read_with(cx, |view, _| view.reveal_state())
            .unwrap_or_default();
        checks.check(
            &format!("dragging selects across blocks and reveals them ({revealed} revealed)"),
            revealed == 3 && selection.is_some_and(|selection| Some(selection.head) == target),
        );
        window.update(cx, |view, _, _| view.end_drag()).ok();
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=scrollbar`, on `10k.md` (#88): the thumb starts at the top,
/// keeps its size and only moves down while paging through, reaches the
/// bottom at the end, and dragging it to the middle shows the middle.
/// Clicking the track pages, and the setting hides it.
fn smoke_scrollbar(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::document_view::PageDown;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let thumb = |cx: &mut AsyncApp| {
            window
                .update(cx, |view, window, cx| view.scrollbar_thumb(window, cx))
                .ok()
                .flatten()
        };
        let top_item = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.scroll_top().map(|(item, _)| item))
                .ok()
                .flatten()
                .unwrap_or(0)
        };
        let first = thumb(cx);
        checks.check(
            &format!("the thumb starts at the top ({first:?})"),
            first.is_some_and(|(top, height, track)| top == 0. && height < track),
        );

        let mut tops = Vec::new();
        let mut heights = Vec::new();
        for _ in 0..40 {
            dispatch(window, cx, Box::new(PageDown));
            pause(cx, 3).await;
            if let Some((top, height, _)) = thumb(cx) {
                tops.push(top);
                heights.push(height);
            }
        }
        let steady = heights
            .iter()
            .all(|height| (height - heights[0]).abs() < 0.5);
        let down = tops.windows(2).all(|pair| pair[1] >= pair[0] - 0.5);
        checks.check(
            &format!(
                "paging keeps the thumb's size and moves it down ({:?} to {:?})",
                tops.first(),
                tops.last()
            ),
            tops.len() == 40 && steady && down && tops.last() > tops.first(),
        );

        // With no caret, ⌘↓ scrolls to the end (#99).
        dispatch(window, cx, Box::new(MoveDocumentEnd));
        pause(cx, 20).await;
        let end = thumb(cx);
        checks.check(
            &format!("at the end the thumb is at the bottom ({end:?})"),
            end.is_some_and(|(top, height, track)| (top + height - track).abs() < 1.),
        );

        // Drag the thumb to the middle of the track.
        if let Some((_, height, track)) = end {
            window
                .update(cx, |view, window, cx| {
                    view.grab_thumb(window);
                    view.drag_thumb_to((track - height) / 2., window, cx);
                    view.release_thumb(cx);
                })
                .ok();
        }
        pause(cx, 10).await;
        let items = window
            .read_with(cx, |view, _| view.item_count())
            .unwrap_or(1);
        let middle = top_item(cx);
        checks.check(
            &format!("dragging to the middle shows the middle (item {middle} of {items})"),
            middle * 10 > items * 3 && middle * 10 < items * 7,
        );

        // Click the track below the thumb: a page down.
        let track = window
            .read_with(cx, |view, _| view.scrollbar_track())
            .ok()
            .flatten();
        let before = window
            .read_with(cx, |view, _| view.scroll_top())
            .ok()
            .flatten();
        if let Some(track) = track {
            let below = gpui::point(track.center().x, track.bottom() - gpui::px(4.));
            mouse_click(window, cx, below).await;
        }
        pause(cx, 5).await;
        let after = window
            .read_with(cx, |view, _| view.scroll_top())
            .ok()
            .flatten();
        checks.check(
            &format!("clicking the track pages down ({before:?} -> {after:?})"),
            matches!((before, after), (Some((a, _)), Some((b, _))) if b > a),
        );

        // From the top, drag the thumb to the bottom a little at a time, so
        // the list draws (and so measures) every item on the way, and hold
        // it there: the document only ever moves down. A drag that starts
        // estimated must stay so although every height becomes known: GPUI
        // froze the list's height when it started, and that's only the
        // height of what had been drawn.
        dispatch(window, cx, Box::new(MoveDocumentStart));
        pause(cx, 10).await;
        let start = thumb(cx);
        let estimated = window
            .update(cx, |view, window, _| {
                view.grab_thumb(window);
                !view.scrollbar_exact(window)
            })
            .unwrap_or(false);
        let mut offsets = Vec::new();
        if let Some((top, height, track)) = start {
            let steps = 400;
            for step in 0..steps + 3 {
                let to = top + (track - height - top) * (step.min(steps) as f32 / steps as f32);
                window
                    .update(cx, |view, window, cx| view.drag_thumb_to(to, window, cx))
                    .ok();
                pause(cx, 1).await;
                offsets.extend(
                    window
                        .read_with(cx, |view, _| view.scroll_top())
                        .ok()
                        .flatten(),
                );
            }
        }
        window.update(cx, |view, _, cx| view.release_thumb(cx)).ok();
        let back = offsets.windows(2).position(|pair| pair[1] < pair[0]);
        checks.check(
            &format!(
                "dragging to the bottom never scrolls back (estimated {estimated}, back at step \
                 {back:?}, ended at {:?})",
                offsets.last()
            ),
            estimated && offsets.len() == 403 && back.is_none(),
        );

        cx.update(|cx| crate::app_settings::set_or_report("scrollbar", false, cx));
        pause(cx, 10).await;
        checks.check("the setting hides it", thumb(cx).is_none());
        cx.update(|cx| crate::app_settings::set_or_report("scrollbar", true, cx));
        pause(cx, 10).await;
        checks.check("and shows it again", thumb(cx).is_some());
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=scrollbar-drag`, on `showcase.md` (#88): a short
/// document's thumb is exact, and dragging it from top to bottom moves the
/// document on every step, all the way to the end, without pausing.
fn smoke_scrollbar_drag(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let exact = window
            .update(cx, |view, window, _| view.scrollbar_exact(window))
            .unwrap_or(false);
        checks.check("a short document's thumb is exact", exact);
        let thumb = window
            .update(cx, |view, window, cx| view.scrollbar_thumb(window, cx))
            .ok()
            .flatten();
        let Some((_, height, track)) = thumb else {
            checks.check("the showcase has a scrollbar", false);
            return checks.finish(cx);
        };
        let offset = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.scroll_offset())
                .unwrap_or(0.)
        };
        window
            .update(cx, |view, window, _| view.grab_thumb(window))
            .ok();
        let steps = 40;
        let mut offsets = vec![offset(cx)];
        for step in 1..=steps {
            window
                .update(cx, |view, window, cx| {
                    view.drag_thumb_to((track - height) * step as f32 / steps as f32, window, cx)
                })
                .ok();
            pause(cx, 1).await;
            offsets.push(offset(cx));
        }
        window.update(cx, |view, _, cx| view.release_thumb(cx)).ok();
        pause(cx, 5).await;
        let stalls: Vec<usize> = offsets
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[1] <= pair[0])
            .map(|(step, _)| step + 1)
            .collect();
        checks.check(
            &format!("every step of the drag scrolls (stalled at steps {stalls:?})"),
            stalls.is_empty(),
        );
        let end = window
            .update(cx, |view, window, cx| view.scrollbar_thumb(window, cx))
            .ok()
            .flatten();
        checks.check(
            &format!("after the drag the thumb stays at the bottom ({end:?})"),
            end.is_some_and(|(top, height, track)| (top + height - track).abs() < 1.),
        );

        // With the mouse: press on the thumb, then drag up with the pointer
        // off the window's right edge, and let go out there. The drag
        // follows the pointer wherever it is, and ends.
        use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
        let bar = window
            .read_with(cx, |view, _| view.scrollbar_track())
            .ok()
            .flatten();
        if let (Some(bar), Some((top, height, _))) = (bar, end) {
            let send = |cx: &mut AsyncApp, event: gpui::PlatformInput| {
                gpui::AnyWindowHandle::from(window)
                    .update(cx, |_, window, cx| {
                        window.dispatch_event(event, cx);
                    })
                    .ok();
            };
            let on_thumb = gpui::point(bar.center().x, bar.top() + gpui::px(top + height / 2.));
            send(
                cx,
                gpui::PlatformInput::MouseMove(MouseMoveEvent {
                    position: on_thumb,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                }),
            );
            pause(cx, 2).await;
            send(
                cx,
                gpui::PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: on_thumb,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }),
            );
            pause(cx, 2).await;
            let before = offset(cx);
            let outside = gpui::point(bar.right() + gpui::px(40.), bar.center().y);
            send(
                cx,
                gpui::PlatformInput::MouseMove(MouseMoveEvent {
                    position: outside,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }),
            );
            pause(cx, 3).await;
            let after = offset(cx);
            checks.check(
                &format!("dragging outside the window still scrolls ({before} -> {after})"),
                after < before - 100.,
            );
            send(
                cx,
                gpui::PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: outside,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }),
            );
            pause(cx, 3).await;
            let dragging = window
                .read_with(cx, |view, _| view.scrollbar_dragging())
                .unwrap_or(true);
            checks.check("letting go outside the window ends the drag", !dragging);
        } else {
            checks.check("the thumb can be found to drag with the mouse", false);
        }
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=reading`, on `anchors.md` (#99, #100): with no caret, ↑
/// and ↓ glide a few lines, holding ↓ scrolls without steps and stops
/// soon after it's let go, the wheel takes over, and ↓ at the end stays
/// there; ⌘↑ and ⌘↓ scroll to the ends, without placing a caret; → starts editing the first block on screen, not the document's
/// start; and after Escape, ↓ scrolls from wherever the window is.
fn smoke_reading(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::document_view::PageDown;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let offset = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.scroll_offset())
                .unwrap_or(0.)
        };
        let caret = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.has_caret())
                .unwrap_or(true)
        };
        let press = async |action: Box<dyn Action>, cx: &mut AsyncApp| {
            dispatch(window, cx, action);
            pause(cx, 5).await;
        };
        checks.check("a newly opened document has no caret", !caret(cx));
        for _ in 0..2 {
            press(Box::new(PageDown), cx).await;
        }
        let start = offset(cx);

        // The keys themselves, since a key repeating is told from a new
        // press by the key going up (#100).
        let key = |cx: &mut AsyncApp, name: &str, held: bool| {
            let event = gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse(name).unwrap(),
                is_held: held,
                prefer_character_input: false,
            });
            gpui::AnyWindowHandle::from(window)
                .update(cx, |_, window, cx| {
                    window.dispatch_event(event, cx);
                })
                .ok();
        };
        let key_up = |cx: &mut AsyncApp, name: &str| {
            let event = gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse(name).unwrap(),
            });
            gpui::AnyWindowHandle::from(window)
                .update(cx, |_, window, cx| {
                    window.dispatch_event(event, cx);
                })
                .ok();
        };
        let scrolling = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.line_scrolling())
                .unwrap_or(false)
        };
        let settle = async |cx: &mut AsyncApp| {
            wait_until(cx, |cx| !scrolling(cx)).await;
            pause(cx, 2).await;
        };

        // A press glides: part of the way after a frame, all of it once
        // it settles.
        key(cx, "down", false);
        key_up(cx, "down");
        let first = after_frames(window, cx, 1, |view| view.scroll_offset())
            .await
            .unwrap_or(0.);
        settle(cx).await;
        let down = offset(cx);
        checks.check(
            &format!("↓ glides a few lines ({start} -> {first} -> {down})"),
            first > start && first < start + 39. && (down - start - 40.).abs() < 1. && !caret(cx),
        );
        key(cx, "up", false);
        key_up(cx, "up");
        settle(cx).await;
        let up = offset(cx);
        checks.check(
            &format!("↑ scrolls back ({down} -> {up})"),
            (up - start).abs() < 1. && !caret(cx),
        );

        // Holding ↓: the key repeats every other frame. Every frame moves
        // the document, none by a whole step; letting go stops it soon.
        key(cx, "down", false);
        let mut offsets = vec![offset(cx)];
        for frame in 0..60 {
            if frame % 2 == 1 {
                key(cx, "down", true);
            }
            offsets.extend(after_frames(window, cx, 1, |view| view.scroll_offset()).await);
        }
        let moves: Vec<f32> = offsets.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let still = moves.iter().filter(|&&moved| moved <= 0.).count();
        let biggest = moves.iter().copied().fold(0., f32::max);
        checks.check(
            &format!(
                "holding ↓ scrolls without steps (frames still {still}, biggest move {biggest:.1})"
            ),
            moves.len() == 60 && still == 0 && biggest < 80.,
        );
        let held = offset(cx);
        key_up(cx, "down");
        let stopped = wait_for(cx, Duration::from_millis(500), |cx| !scrolling(cx)).await;
        let glide = offset(cx) - held;
        checks.check(
            &format!("letting go stops it promptly (glided {glide:.1})"),
            stopped && glide < 80.,
        );

        // The wheel takes over from a held key at once.
        key(cx, "down", false);
        for _ in 0..6 {
            key(cx, "down", true);
            pause(cx, 2).await;
        }
        gpui::AnyWindowHandle::from(window)
            .update(cx, |_, window, cx| {
                let size = window.viewport_size();
                window.dispatch_event(
                    gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                        position: gpui::point(size.width / 2., size.height / 2.),
                        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(100.))),
                        modifiers: gpui::Modifiers::default(),
                        touch_phase: gpui::TouchPhase::Moved,
                    }),
                    cx,
                );
            })
            .ok();
        pause(cx, 3).await;
        let wheeled = !scrolling(cx);
        key_up(cx, "down");
        checks.check("the wheel takes over from ↓", wheeled);

        // At the end, ↓ doesn't go past it.
        press(Box::new(MoveDocumentEnd), cx).await;
        let bottom = offset(cx);
        key(cx, "down", false);
        key_up(cx, "down");
        settle(cx).await;
        checks.check(
            &format!("↓ at the end stays there ({bottom} -> {})", offset(cx)),
            (offset(cx) - bottom).abs() < 1.,
        );

        press(Box::new(MoveDocumentEnd), cx).await;
        let end = window
            .update(cx, |view, window, cx| view.scrollbar_thumb(window, cx))
            .ok()
            .flatten();
        checks.check(
            &format!("⌘↓ scrolls to the end ({end:?})"),
            end.is_some_and(|(top, height, track)| (top + height - track).abs() < 1.) && !caret(cx),
        );
        press(Box::new(MoveDocumentStart), cx).await;
        checks.check("⌘↑ scrolls to the top", offset(cx) == 0. && !caret(cx));

        for _ in 0..2 {
            press(Box::new(PageDown), cx).await;
        }
        let first = window
            .read_with(cx, |view, _| view.scroll_top().map(|(item, _)| item))
            .ok()
            .flatten();
        press(Box::new(MoveRight), cx).await;
        pause(cx, 5).await;
        let (_, selection) = window
            .read_with(cx, |view, _| view.reveal_state())
            .unwrap_or_default();
        let placed = window
            .read_with(cx, |view, _| {
                selection.map(|selection| view.item_at(selection.head))
            })
            .ok()
            .flatten();
        checks.check(
            &format!("→ starts editing the first block on screen (item {placed:?}, top {first:?})"),
            placed.is_some() && placed == first,
        );

        // Escape, scroll away from the block, then ↓.
        press(Box::new(Unreveal), cx).await;
        for _ in 0..3 {
            press(Box::new(PageDown), cx).await;
        }
        let before = offset(cx);
        press(Box::new(MoveDown), cx).await;
        settle(cx).await;
        let after = offset(cx);
        checks.check(
            &format!("after Escape, ↓ scrolls from where the window is ({before} -> {after})"),
            (after - before - 40.).abs() < 1. && !caret(cx),
        );
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=caret`, on `anchors.md` and `showcase.md` (#98, #102):
/// stepping through the document a block or a line at a time, down and
/// back up, or jumping to either end, keeps the caret on screen after
/// every step, in the first frame drawn after it (so scrolling a frame
/// late fails).
fn smoke_caret(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let item_count = window
            .read_with(cx, |view, _| view.item_count())
            .unwrap_or(0);
        // The other movements, checked for the same problem: arrows a line
        // at a time (twice as many steps, as blank lines count), and the
        // document's ends.
        let steps = [
            (
                "Option-Down",
                Box::new(MoveBlockDown) as Box<dyn Action>,
                false,
                item_count,
            ),
            ("Option-Up", Box::new(MoveBlockUp), false, item_count),
            (
                "Option-Shift-Down",
                Box::new(SelectBlockDown),
                true,
                item_count,
            ),
            ("Down", Box::new(MoveDown), false, item_count * 2),
            ("Up", Box::new(MoveUp), false, item_count * 2),
            ("Cmd-Down", Box::new(MoveDocumentEnd), false, 1),
            ("Cmd-Up", Box::new(MoveDocumentStart), false, 1),
        ];
        for (name, action, extend, steps) in steps {
            let mut off_screen = Vec::new();
            for step in 0..steps {
                dispatch(window, cx, action.boxed_clone());
                let on_screen = after_frames(window, cx, 1, |view| view.caret_on_screen())
                    .await
                    .flatten();
                if on_screen != Some(true) {
                    off_screen.push(step);
                }
            }
            checks.check(
                &format!("{name} keeps the caret on screen (off at steps {off_screen:?})"),
                off_screen.is_empty(),
            );
            if extend {
                dispatch(window, cx, Box::new(MoveDocumentStart));
                pause(cx, 5).await;
            }
        }
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=images`, on `showcase.md` with its `images` folder (#92):
/// the showcase's PNG and SVG load at their natural sizes, the missing one
/// doesn't, and the images are reported to VoiceOver by their alt text.
fn smoke_images(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let Some(folder) = window
            .read_with(cx, |view, _| {
                view.document_path()
                    .and_then(Path::parent)
                    .map(Path::to_owned)
            })
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        window
            .update(cx, |view, _, cx| view.go_to_anchor("images", cx))
            .ok();
        for (name, expected) in [
            ("gradient.png", Some((480, 160))),
            ("badge.svg", Some((96, 20))),
            ("missing.png", None),
        ] {
            let resource = gpui::Resource::Path(folder.join("images").join(name).into());
            let mut loaded = None;
            wait_until(cx, |cx| {
                loaded = cx.update(|cx| cx.fetch_asset::<gpui::ImgResourceLoader>(&resource));
                loaded.is_some()
            })
            .await;
            let size = match &loaded {
                Some(Ok(image)) => {
                    let (width, height) = crate::rendered_block::natural_size(image, name);
                    Some((width as i32, height as i32))
                }
                _ => None,
            };
            checks.check(
                &format!("{name} loads as {expected:?} ({size:?})"),
                size == expected,
            );
        }
        let tree = a11y_tree(window.into(), cx).await;
        for label in ["A blue gradient", "tests: pass", "no such image"] {
            checks.check(
                &format!("the {label:?} image is reported"),
                tree.contains("\"Image\"") && tree.contains(label),
            );
        }
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=web-images`, on `showcase.md` with its `images` folder
/// (#97): a document with a web image, served from a local server, asks
/// before loading it; Load Images loads it and the bar goes away;
/// reopening the document asks again, and closing the bar leaves the
/// image unloaded. A document with no web images doesn't ask.
fn smoke_web_images(window: WindowHandle<DocumentView>, cx: &mut App) {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        checks.check(
            "a document with no web images doesn't ask",
            window
                .read_with(cx, |view, _| !view.web_images_asked())
                .unwrap_or(false),
        );
        let Some(folder) = window
            .read_with(cx, |view, _| {
                view.document_path()
                    .and_then(Path::parent)
                    .map(Path::to_owned)
            })
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        // A web server with one image, counting the requests for it.
        let png = std::fs::read(folder.join("images/gradient.png")).unwrap_or_default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("local server");
        let port = listener.local_addr().expect("server address").port();
        let requests = Arc::new(AtomicUsize::new(0));
        let served = requests.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                    }
                }
                served.fetch_add(1, Ordering::SeqCst);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(header.as_bytes()).ok();
                stream.write_all(&png).ok();
            }
        });
        let url = format!("http://127.0.0.1:{port}/gradient.png");
        let web = folder.join("web.md");
        std::fs::write(&web, format!("# Web\n\n![A web gradient]({url})\n")).ok();
        let open = |cx: &mut AsyncApp| {
            window
                .update(cx, |view, window, cx| {
                    view.open_path(web.clone(), window, cx)
                })
                .ok();
        };
        let asked = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.web_images_asked())
                .unwrap_or(false)
        };

        open(cx);
        checks.check("the bar asks", wait_until(cx, |cx| asked(cx)).await);
        let tree = a11y_tree(window.into(), cx).await;
        checks.check(
            "VoiceOver reads the bar and its buttons",
            [
                "This document has images from the web.",
                "Load Images",
                "Don't load images",
            ]
            .iter()
            .all(|label| tree.contains(label)),
        );
        pause(cx, 10).await;
        checks.check(
            "nothing is fetched before Load Images",
            requests.load(Ordering::SeqCst) == 0,
        );

        window
            .update(cx, |view, window, cx| {
                view.answer_web_images_bar(true, window, cx)
            })
            .ok();
        checks.check("Load Images closes the bar", !asked(cx));
        let resource = gpui::Resource::Uri(url.clone().into());
        let mut loaded = None;
        wait_until(cx, |cx| {
            loaded = cx.update(|cx| cx.fetch_asset::<gpui::ImgResourceLoader>(&resource));
            loaded.is_some()
        })
        .await;
        let size = match &loaded {
            Some(Ok(image)) => {
                let (width, height) = crate::rendered_block::natural_size(image, &url);
                Some((width as i32, height as i32))
            }
            _ => None,
        };
        checks.check(
            &format!("the web image loads ({size:?})"),
            size == Some((480, 160)) && requests.load(Ordering::SeqCst) == 1,
        );
        checks.check(
            "the fetched image is kept for printing",
            crate::web_images::fetched_image(&url).is_some(),
        );

        open(cx);
        checks.check(
            "reopening the document asks again",
            wait_until(cx, |cx| asked(cx)).await,
        );
        window
            .update(cx, |view, window, cx| {
                view.answer_web_images_bar(false, window, cx)
            })
            .ok();
        checks.check("closing the bar closes it", !asked(cx));
        std::fs::remove_file(&web).ok();
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=anchors`, on `anchors.md` (#91, #96): clicking a
/// `#setup` link scrolls its heading to the top; a link to a missing
/// heading stays put; `#middle` reaches an `<a id>`; and clicking
/// `other.md#deep-dive` opens other.md scrolled to that heading.
fn smoke_anchors(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let Some(path) = window
            .read_with(cx, |view, _| view.document_path().map(Path::to_owned))
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        let mut other_text = String::from("# Other\n\n");
        for index in 0..60 {
            other_text.push_str(&format!("Paragraph {index}.\n\n"));
        }
        other_text.push_str("## Deep dive\n\n");
        // Enough after it that it can scroll to the top.
        for index in 0..60 {
            other_text.push_str(&format!("More {index}.\n\n"));
        }
        let other = path.with_file_name("other.md");
        std::fs::write(&other, other_text).ok();

        let top_item = |window: WindowHandle<DocumentView>, cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.scroll_top().map(|(item, _)| item))
                .ok()
                .flatten()
        };
        let click_link = async |index: usize, cx: &mut AsyncApp| {
            let origin = window
                .update(cx, |view, window, _| view.text_origin(index, window))
                .ok()
                .flatten();
            if let Some(origin) = origin {
                let position = origin + gpui::point(gpui::px(12.), gpui::px(10.));
                mouse_click(window, cx, position).await;
            }
            pause(cx, 10).await;
        };

        // Item 1 links to #setup; the heading is item 63.
        click_link(1, cx).await;
        let top = top_item(window, cx);
        checks.check(
            &format!("#setup scrolls to the heading (top item {top:?})"),
            top == Some(63),
        );

        window
            .update(cx, |view, window, cx| {
                view.follow_link("#no-such-heading", window, cx)
            })
            .ok();
        pause(cx, 5).await;
        checks.check(
            "a missing heading stays put",
            top_item(window, cx) == Some(63),
        );

        // An HTML anchor, with and without GitHub's prefix (#96). It's on
        // the 31st filler paragraph, item 33.
        for link in ["#middle", "#user-content-middle"] {
            window
                .update(cx, |view, window, cx| {
                    view.follow_link("#anchors", window, cx)
                })
                .ok();
            pause(cx, 5).await;
            window
                .update(cx, |view, window, cx| view.follow_link(link, window, cx))
                .ok();
            pause(cx, 10).await;
            let top = top_item(window, cx);
            checks.check(
                &format!("{link} scrolls to the <a id> (top item {top:?})"),
                top == Some(33),
            );
        }

        // Back to the top, then follow item 2's link into other.md.
        window
            .update(cx, |view, window, cx| {
                view.follow_link("#anchors", window, cx)
            })
            .ok();
        pause(cx, 10).await;
        checks.check("#anchors goes back up", top_item(window, cx) == Some(0));
        click_link(2, cx).await;
        let opened = wait_until(cx, |cx| {
            cx.update(|cx| {
                cx.windows()
                    .into_iter()
                    .filter_map(|handle| handle.downcast::<DocumentView>())
                    .find(|handle| {
                        handle
                            .read(cx)
                            .ok()
                            .and_then(|view| view.document_path())
                            .is_some_and(|open| open.file_name() == other.file_name())
                    })
                    .is_some()
            })
        })
        .await;
        pause(cx, 10).await;
        let other_window = cx.update(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|handle| handle.downcast::<DocumentView>())
                .find(|handle| {
                    handle
                        .read(cx)
                        .ok()
                        .and_then(|view| view.document_path())
                        .is_some_and(|open| open.file_name() == other.file_name())
                })
        });
        let other_top = other_window.and_then(|other| top_item(other, cx));
        checks.check(
            &format!("other.md#deep-dive opens it at the heading (top item {other_top:?})"),
            opened && other_top == Some(61),
        );

        // Following it again from the first window, with other.md already
        // open, scrolls there again.
        if let Some(other_window) = other_window {
            other_window
                .update(cx, |view, window, cx| {
                    view.follow_link("#other", window, cx)
                })
                .ok();
            pause(cx, 10).await;
            window
                .update(cx, |view, window, cx| {
                    view.follow_link("other.md#deep-dive", window, cx)
                })
                .ok();
            pause(cx, 20).await;
            let again = top_item(other_window, cx);
            checks.check(
                &format!("an already open file scrolls too (top item {again:?})"),
                again == Some(61),
            );
        }
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=footnotes`, on `footnotes.md` (#90): the notes are drawn
/// in rows after the document's blocks; clicking a reference scrolls to its
/// note; a back-link scrolls to its reference, even deep in a long list;
/// and clicking a note's text reveals its definition where it is.
fn smoke_footnotes(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let (first, count) = window
            .read_with(cx, |view, _| view.note_rows())
            .unwrap_or_default();
        // The heading, the paragraph with the first reference, 30 fillers,
        // three definitions, the list and 30 more fillers.
        checks.check(
            &format!("three notes after 66 items (rows {first}+{count})"),
            (first, count) == (66, 3),
        );
        let on_screen = |row: usize, cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.row_on_screen(row))
                .unwrap_or(false)
        };
        let caret = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.reveal_state().1)
                .ok()
                .flatten()
        };

        // The paragraph starts with its reference.
        let origin = window
            .update(cx, |view, window, _| view.text_origin(1, window))
            .ok()
            .flatten();
        if let Some(origin) = origin {
            mouse_click(window, cx, origin + gpui::point(gpui::px(2.), gpui::px(12.))).await;
        }
        pause(cx, 10).await;
        checks.check(
            "clicking a reference scrolls to its note",
            on_screen(first, cx) && !on_screen(1, cx) && caret(cx).is_none(),
        );

        window
            .update(cx, |view, window, cx| {
                view.follow_link("#fnref-list", window, cx)
            })
            .ok();
        pause(cx, 10).await;
        let top = window
            .read_with(cx, |view, _| view.scroll_top())
            .ok()
            .flatten();
        // The list's items are about 26 px high, and the reference is in
        // its 36th.
        checks.check(
            &format!("a back-link scrolls to its reference in the list (top {top:?})"),
            top.is_some_and(|(item, offset)| {
                item == 35 && offset > gpui::px(700.) && offset < gpui::px(1100.)
            }),
        );

        window
            .update(cx, |view, window, cx| {
                view.follow_link("#fn-list", window, cx)
            })
            .ok();
        pause(cx, 10).await;
        checks.check("#fn-list scrolls to the note", on_screen(first + 1, cx));

        // The second note has no rule above it; click its text.
        let origin = window
            .update(cx, |view, window, _| view.text_origin(first + 1, window))
            .ok()
            .flatten();
        if let Some(origin) = origin {
            mouse_click(window, cx, origin + gpui::point(gpui::px(120.), gpui::px(10.))).await;
        }
        pause(cx, 15).await;
        let head = caret(cx).map(|selection| selection.head);
        let item = head.and_then(|head| window.read_with(cx, |view, _| view.item_at(head)).ok());
        let shown = window
            .read_with(cx, |view, _| view.caret_on_screen())
            .ok()
            .flatten();
        checks.check(
            &format!(
                "clicking a note reveals its definition where it is (item {item:?}, on screen {shown:?})"
            ),
            item == Some(33) && shown == Some(true),
        );
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=clicks`, on `clicks.md`: clicking a link to a Markdown file
/// opens it in a tab and leaves the link's block rendered; clicking a
/// checkbox toggles it, as one undo step, and leaves its list rendered;
/// clicking plain text still reveals its block.
fn smoke_clicks(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let Some(path) = window
            .read_with(cx, |view, _| view.document_path().map(Path::to_owned))
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        let other = path.with_file_name("other.md");
        std::fs::write(&other, "# Other\n").ok();
        let origin = |cx: &mut AsyncApp, index: usize| {
            window
                .update(cx, |view, window, _| view.text_origin(index, window))
                .ok()
                .flatten()
        };
        let text_position =
            |origin: gpui::Point<gpui::Pixels>| origin + gpui::point(gpui::px(12.), gpui::px(10.));

        // The first block starts with a link to other.md.
        let Some(first) = origin(cx, 0) else {
            checks.check("the first block is laid out", false);
            return checks.finish(cx);
        };
        mouse_click(window, cx, text_position(first)).await;
        let opened = wait_until(cx, |cx| {
            cx.update(|cx| {
                cx.windows().into_iter().any(|handle| {
                    handle
                        .downcast::<DocumentView>()
                        .and_then(|handle| handle.read(cx).ok())
                        .and_then(|view| view.document_path())
                        .is_some_and(|open| open.file_name() == other.file_name())
                })
            })
        })
        .await;
        checks.check("clicking a Markdown link opens the file", opened);
        let (revealed, selection) = window
            .read_with(cx, |view, _| view.reveal_state())
            .unwrap_or_default();
        checks.check(
            &format!("the link's block stays rendered ({revealed} revealed, {selection:?})"),
            revealed == 0 && selection.is_none(),
        );

        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        pause(cx, 10).await;

        // The third block is a task list.
        let text = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.text())
                .unwrap_or_default()
        };
        if let Some(list) = origin(cx, 2) {
            mouse_click(window, cx, text_position(list)).await;
        }
        pause(cx, 5).await;
        let (revealed, selection) = window
            .read_with(cx, |view, _| view.reveal_state())
            .unwrap_or_default();
        let dirty = window
            .read_with(cx, |view, _| view.is_dirty())
            .unwrap_or(false);
        checks.check(
            &format!("clicking a checkbox ticks it ({revealed} revealed, {selection:?})"),
            text(cx).ends_with("- [x] Task\n") && revealed == 0 && selection.is_none() && dirty,
        );
        dispatch(window, cx, Box::new(Undo));
        pause(cx, 5).await;
        checks.check("undo unticks it", text(cx).ends_with("- [ ] Task\n"));
        dispatch(window, cx, Box::new(Unreveal));
        pause(cx, 5).await;

        // Plain text in the second block reveals it.
        let start = window
            .read_with(cx, |view, _| view.item_start(1))
            .ok()
            .flatten();
        if let Some(second) = origin(cx, 1) {
            mouse_click(window, cx, text_position(second)).await;
        }
        pause(cx, 5).await;
        let (revealed, selection) = window
            .read_with(cx, |view, _| view.reveal_state())
            .unwrap_or_default();
        checks.check(
            &format!("clicking plain text reveals its block ({selection:?})"),
            revealed == 1
                && start
                    .is_some_and(|start| selection.is_some_and(|selection| selection.head > start)),
        );
        checks.finish(cx);
    })
    .detach();
}

/// Turns on `window`'s accessibility tree, as VoiceOver does by asking
/// the window's view about its contents, and returns GPUI's description of
/// the tree once it's built.
async fn a11y_tree(window: gpui::AnyWindowHandle, cx: &mut AsyncApp) -> String {
    window
        .update(cx, |_, window, _| {
            let Some(ns_window) = crate::native_tabs::ns_window(window) else {
                return;
            };
            if let Some(view) = ns_window.contentView() {
                // SAFETY: `accessibilityChildren` is an NSAccessibility
                // method every NSView has; it takes no arguments and returns
                // an object (an array, or nil), which is dropped unused.
                let _: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
                    unsafe { objc2::msg_send![&*view, accessibilityChildren] };
            }
        })
        .ok();
    let mut tree = String::new();
    for _ in 0..50 {
        pause(cx, 5).await;
        window
            .update(cx, |_, window, cx| {
                // Draw a frame with the tree in it.
                window.refresh();
                let _ = cx;
                tree = window.debug_a11y_tree_json().unwrap_or_default();
            })
            .ok();
        if tree.contains("\"Document\"") || tree.contains("\"Dialog\"") {
            break;
        }
    }
    tree
}

/// `KERTAS_SMOKE=a11y`, on `a11y.md`: what VoiceOver is told about the
/// document, the tab strip and the settings sheet (N5).
fn smoke_a11y(window: WindowHandle<DocumentView>, cx: &mut App) {
    use crate::menus::OpenSettings;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        // A second tab, so there's a tab strip.
        dispatch(window, cx, Box::new(New));
        pause(cx, 20).await;
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        pause(cx, 20).await;
        let tree = a11y_tree(window.into(), cx).await;
        if let Ok(path) = std::env::var("KERTAS_A11Y_DUMP") {
            std::fs::write(&path, &tree).ok();
        }
        // GPUI's description of the tree is pretty-printed JSON.
        let has = |tree: &str, fields: &[&str]| {
            tree.split("\"aria\": {")
                .any(|node| fields.iter().all(|field| node.contains(field)))
        };
        checks.check(
            "the document is named after its file",
            has(&tree, &["\"role\": \"Document\"", "\"label\": \"a11y.md\""]),
        );
        checks.check(
            "headings have their level",
            has(
                &tree,
                &[
                    "\"role\": \"Heading\"",
                    "\"label\": \"Title\"",
                    "\"level\": 1",
                ],
            ),
        );
        checks.check(
            "paragraphs and code are labels",
            has(
                &tree,
                &["\"role\": \"Label\"", "\"value\": \"A paragraph.\""],
            ) && has(&tree, &["\"role\": \"Label\"", "\"value\": \"code\""]),
        );
        checks.check(
            "task checkboxes are named and checked",
            has(
                &tree,
                &[
                    "\"role\": \"CheckBox\"",
                    "\"label\": \"Done task\"",
                    "\"toggled\": \"True\"",
                ],
            ) && has(
                &tree,
                &[
                    "\"role\": \"CheckBox\"",
                    "\"label\": \"Open task\"",
                    "\"toggled\": \"False\"",
                ],
            ),
        );
        checks.check(
            "tabs are named, and the current one selected",
            has(&tree, &["\"role\": \"TabList\""])
                && has(
                    &tree,
                    &[
                        "\"role\": \"Tab\"",
                        "\"label\": \"a11y.md\"",
                        "\"selected\": true",
                    ],
                )
                && has(
                    &tree,
                    &[
                        "\"role\": \"Tab\"",
                        "\"label\": \"Untitled\"",
                        "\"selected\": false",
                    ],
                )
                && has(
                    &tree,
                    &["\"role\": \"Button\"", "\"label\": \"Close a11y.md\""],
                ),
        );

        dispatch(window, cx, Box::new(OpenSettings));
        pause(cx, 20).await;
        let settings = cx.update(|cx| {
            cx.windows()
                .into_iter()
                .find(|other| other.downcast::<DocumentView>().is_none())
        });
        let tree = match settings {
            Some(settings) => a11y_tree(settings, cx).await,
            None => String::new(),
        };
        if let Ok(path) = std::env::var("KERTAS_A11Y_DUMP") {
            std::fs::write(format!("{path}.settings"), &tree).ok();
        }
        checks.check(
            "the settings sheet is a dialog with named choices",
            has(&tree, &["\"role\": \"Dialog\"", "\"label\": \"Settings\""])
                && has(
                    &tree,
                    &["\"role\": \"RadioGroup\"", "\"label\": \"Appearance\""],
                )
                && has(
                    &tree,
                    &[
                        "\"role\": \"RadioButton\"",
                        "\"label\": \"Match System\"",
                        "\"toggled\": \"True\"",
                    ],
                )
                && has(
                    &tree,
                    &["\"role\": \"CheckBox\"", "\"label\": \"Editing hint\""],
                )
                && has(
                    &tree,
                    &[
                        "\"role\": \"Button\"",
                        "\"label\": \"Increase default text size",
                    ],
                )
                && has(
                    &tree,
                    &["\"role\": \"DefaultButton\"", "\"label\": \"Done\""],
                ),
        );
        if let Some(settings) = settings {
            dispatch(settings, cx, Box::new(CloseWindow));
        }
        pause(cx, 10).await;
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=print`, with `KERTAS_PRINT_PDF` set: prints the document to
/// that PDF without any panels.
fn smoke_print(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let path =
            std::path::PathBuf::from(std::env::var_os("KERTAS_PRINT_PDF").unwrap_or_default());
        let model = window.read_with(cx, |view, _| view.model()).ok().flatten();
        let document = window
            .read_with(cx, |view, _| view.document_path().map(Path::to_owned))
            .ok()
            .flatten();
        let printed = cx.update(|cx| {
            let mtm = objc2::MainThreadMarker::new()?;
            let settings = crate::app_settings::get(cx);
            Some(crate::print::save_pdf(
                &*model?,
                document.as_deref(),
                &settings,
                &path,
                mtm,
            ))
        });
        let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        checks.check(
            &format!("printed to a PDF ({size} bytes)"),
            printed == Some(true) && size > 0,
        );
        checks.finish(cx);
    })
    .detach();
}

/// Waits until `done` holds, for up to two seconds. Returns whether it
/// did.
async fn wait_until(cx: &mut AsyncApp, done: impl FnMut(&mut AsyncApp) -> bool) -> bool {
    wait_for(cx, Duration::from_secs(2), done).await
}

/// Waits until `window` has drawn `frames` frames, then reads the view
/// with `read`, before the next frame (and anything it was told to do
/// before that frame) can change what was drawn.
async fn after_frames<T: 'static>(
    window: WindowHandle<DocumentView>,
    cx: &mut AsyncApp,
    frames: usize,
    read: impl FnOnce(&DocumentView) -> T + 'static,
) -> Option<T> {
    /// Calls `then` at the start of the `hops`th frame from now.
    fn hop(window: &mut gpui::Window, hops: usize, then: Box<dyn FnOnce(&mut App)>) {
        window.on_next_frame(move |window, cx| {
            if hops <= 1 {
                then(cx);
            } else {
                hop(window, hops - 1, then);
            }
        });
    }
    let result = Rc::new(std::cell::RefCell::new(None));
    let slot = result.clone();
    // Frame callbacks run at the start of a frame, before it's drawn: the
    // first one is before the next frame, so wait one more.
    window
        .update(cx, |_, window, cx| {
            let view = cx.entity();
            let then = Box::new(move |cx: &mut App| {
                *slot.borrow_mut() = Some(read(view.read(cx)));
            });
            hop(window, frames + 1, then);
        })
        .ok()?;
    wait_until(cx, |_| result.borrow().is_some()).await;
    result.take()
}

/// Waits until `done` returns true, or `timeout` passes. Returns whether
/// it did.
async fn wait_for(
    cx: &mut AsyncApp,
    timeout: Duration,
    mut done: impl FnMut(&mut AsyncApp) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done(cx) {
            return true;
        }
        pause(cx, 2).await;
    }
    done(cx)
}

/// `KERTAS_SMOKE=disk`: changes made to the file by "another app" (F5). With
/// no unsaved changes it reloads, as one undo step; with unsaved changes
/// the bar asks, and Keep Mine and Reload work; Kertas's own saves aren't
/// reported; a deleted file is reported, and saving puts it back.
fn smoke_disk(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let Some(path) = window
            .read_with(cx, |view, _| view.document_path().map(Path::to_owned))
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        let original = std::fs::read_to_string(&path).unwrap_or_default();
        let state = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| {
                    (view.text(), view.is_dirty(), view.disk_change_shown())
                })
                .unwrap_or_default()
        };

        // Scrolled down, a block added at the top keeps the same text in
        // view.
        for _ in 0..2 {
            dispatch(window, cx, Box::new(crate::document_view::PageDown));
            pause(cx, 5).await;
        }
        let scroll = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.scroll_top())
                .ok()
                .flatten()
        };
        let before = scroll(cx);
        let theirs = format!("# Changed elsewhere\n\n{original}");
        std::fs::write(&path, &theirs).ok();
        let reloaded = wait_until(cx, |cx| state(cx).0 == theirs).await;
        let (_, dirty, bar) = state(cx);
        checks.check(
            "reloads a change with no unsaved changes",
            reloaded && !dirty && !bar,
        );
        let after = scroll(cx);
        checks.check(
            &format!("keeps the scroll position ({before:?} -> {after:?})"),
            matches!((before, after), (Some((a, x)), Some((b, y))) if b == a + 1 && x == y && a > 0),
        );
        dispatch(window, cx, Box::new(Undo));
        pause(cx, 5).await;
        let (text, dirty, _) = state(cx);
        checks.check(
            "undo goes back to before the reload, as unsaved",
            text == original && dirty,
        );
        dispatch(window, cx, Box::new(Redo));
        pause(cx, 5).await;
        checks.check("redo reloads again", state(cx).0 == theirs && !state(cx).1);

        // Editors often save by renaming a new file over the old one.
        let replaced = format!("{theirs}Replaced.\n");
        let temp = path.with_extension("md.tmp");
        std::fs::write(&temp, &replaced).ok();
        std::fs::rename(&temp, &path).ok();
        checks.check(
            "reloads a file replaced by renaming",
            wait_until(cx, |cx| state(cx).0 == replaced).await,
        );

        // With unsaved changes, the bar asks.
        let type_text = |cx: &mut AsyncApp, text: &'static str| {
            window
                .update(cx, |view, window, cx| {
                    view.click(0, false, window, cx);
                    view.replace_text_in_range(None, text, window, cx);
                })
                .ok();
        };
        type_text(cx, "Mine. ");
        let mine = state(cx).0;
        std::fs::write(&path, "Theirs.\n").ok();
        let asked = wait_until(cx, |cx| state(cx).2).await;
        checks.check(
            "asks when there are unsaved changes, keeping them",
            asked && state(cx).0 == mine,
        );
        window.update(cx, |view, _, cx| view.keep_mine(cx)).ok();
        pause(cx, 2).await;
        let (text, dirty, bar) = state(cx);
        checks.check("Keep Mine keeps the text", text == mine && dirty && !bar);

        std::fs::write(&path, "Theirs again.\n").ok();
        wait_until(cx, |cx| state(cx).2).await;
        window
            .update(cx, |view, _, cx| view.reload_pending(cx))
            .ok();
        pause(cx, 2).await;
        let (text, dirty, bar) = state(cx);
        checks.check(
            "Reload replaces unsaved changes",
            text == "Theirs again.\n" && !dirty && !bar,
        );

        // Kertas's own saves aren't reported.
        type_text(cx, "Saved. ");
        dispatch(window, cx, Box::new(Save));
        pause(cx, 20).await;
        let (text, dirty, bar) = state(cx);
        checks.check(
            "Kertas's own save isn't reported",
            text == "Saved. Theirs again.\n" && !dirty && !bar,
        );

        std::fs::remove_file(&path).ok();
        let reported = wait_until(cx, |cx| state(cx).2).await;
        checks.check(
            "a deleted file is reported, keeping the text",
            reported && state(cx).0 == "Saved. Theirs again.\n",
        );
        dispatch(window, cx, Box::new(Save));
        pause(cx, 5).await;
        checks.check(
            "saving puts a deleted file back",
            std::fs::read_to_string(&path).ok().as_deref() == Some("Saved. Theirs again.\n")
                && !state(cx).2,
        );
        checks.finish(cx);
    })
    .detach();
}

/// `KERTAS_SMOKE=autosave`: with `autosave = 1`, edits save after a pause
/// without touching undo; a change made elsewhere isn't saved over; a
/// failed autosave pauses it until a manual save; switching tabs saves;
/// and finally Quit saves without asking. The caller checks that the
/// process ends and the last text was saved.
fn smoke_autosave(window: WindowHandle<DocumentView>, cx: &mut App) {
    use std::os::unix::fs::PermissionsExt;

    cx.spawn(async move |cx| {
        let mut checks = Checks::default();
        pause(cx, 10).await;
        let Some(path) = window
            .read_with(cx, |view, _| view.document_path().map(Path::to_owned))
            .ok()
            .flatten()
        else {
            checks.check("the document has a file", false);
            return checks.finish(cx);
        };
        let original = std::fs::read_to_string(&path).unwrap_or_default();
        cx.update(|cx| crate::app_settings::set("autosave", 1, cx))
            .ok();
        let on_disk = || std::fs::read_to_string(&path).unwrap_or_default();
        let dirty = |cx: &mut AsyncApp| {
            window
                .read_with(cx, |view, _| view.is_dirty())
                .unwrap_or(true)
        };
        let type_text = |cx: &mut AsyncApp, text: &'static str| {
            window
                .update(cx, |view, window, cx| {
                    view.click(0, false, window, cx);
                    view.replace_text_in_range(None, text, window, cx);
                })
                .ok();
        };

        type_text(cx, "Autosaved. ");
        pause(cx, 20).await;
        checks.check(
            "doesn't save straight away",
            dirty(cx) && on_disk() == original,
        );
        let saved = wait_until(cx, |cx| !dirty(cx)).await;
        checks.check(
            "saves after the pause",
            saved && on_disk() == format!("Autosaved. {original}"),
        );
        dispatch(window, cx, Box::new(Undo));
        pause(cx, 5).await;
        let text = window
            .read_with(cx, |view, _| view.text())
            .unwrap_or_default();
        checks.check("undo still undoes the edit", text == original);
        checks.check(
            "and that's saved too",
            wait_until(cx, |cx| !dirty(cx)).await && on_disk() == original,
        );

        // A change made elsewhere is never saved over.
        type_text(cx, "Mine. ");
        std::fs::write(&path, "Theirs.\n").ok();
        pause(cx, 150).await;
        let shown = window
            .read_with(cx, |view, _| view.disk_change_shown())
            .unwrap_or(false);
        checks.check(
            "doesn't save over a change made elsewhere",
            on_disk() == "Theirs.\n" && shown && dirty(cx),
        );
        window.update(cx, |view, _, cx| view.keep_mine(cx)).ok();
        checks.check(
            "saves after Keep Mine",
            wait_until(cx, |cx| !dirty(cx)).await && on_disk() == format!("Mine. {original}"),
        );

        // Switching to another tab saves at once.
        type_text(cx, "Switching. ");
        dispatch(window, cx, Box::new(New));
        pause(cx, 10).await;
        checks.check(
            "switching tabs saves",
            !dirty(cx) && on_disk().starts_with("Switching. "),
        );
        dispatch(window, cx, Box::new(SelectTab1));
        pause(cx, 10).await;

        // A failed autosave pauses autosave until a manual save.
        let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
        let permissions =
            |mode| std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(mode)).ok();
        permissions(0o555);
        type_text(cx, "Failing. ");
        let paused = wait_until(cx, |cx| {
            window
                .read_with(cx, |view, _| view.autosave_paused())
                .unwrap_or(false)
        })
        .await;
        checks.check("a failed autosave pauses autosave", paused && dirty(cx));
        permissions(0o755);
        type_text(cx, "Still paused. ");
        pause(cx, 120).await;
        checks.check("stays paused", dirty(cx));
        dispatch(window, cx, Box::new(Save));
        pause(cx, 5).await;
        let resumed = !window
            .read_with(cx, |view, _| view.autosave_paused())
            .unwrap_or(true);
        checks.check("a manual save resumes it", resumed && !dirty(cx));
        // The alert is still showing; quitting closes it.

        type_text(cx, "Quitting. ");
        checks.report();
        println!("dispatching Quit");
        dispatch(window, cx, Box::new(Quit));
    })
    .detach();
}

/// `KERTAS_SMOKE=quit`: Quit, with an unedited document, exits the app. The
/// caller checks that the process ends.
fn smoke_quit(window: WindowHandle<DocumentView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        pause(cx, 20).await;
        println!("dispatching Quit");
        dispatch(window, cx, Box::new(Quit));
    })
    .detach();
}
