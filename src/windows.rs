//! Document windows: one document per window.
//!
//! By default the windows are native macOS tabs (§8.2's `tabs` layout):
//! windows sharing a tabbing identifier open as tabs of the current window,
//! and macOS provides the tab bar, dragging tabs out into windows, and
//! Window > Merge All Windows. The `windows` layout would leave the
//! identifier unset; that setting arrives with the config file.

use std::path::{Path, PathBuf};

use gpui::{
    App, Bounds, Context, PathPromptOptions, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions, prelude::*, px, size,
};

use crate::document_view::{CloseWindow, DocumentView, New, Open, Quit};

/// Groups document windows into native tabs.
const TABBING_IDENTIFIER: &str = "kami-documents";

/// Registers the app-wide document actions.
pub fn register(cx: &mut App) {
    cx.on_action(|_: &New, cx| {
        open_window(cx, |view, window, cx| view.start_untitled(window, cx));
    });
    cx.on_action(|_: &Open, cx| choose_and_open(cx));
    // Quitting closes each window, which asks about unsaved changes;
    // closing the last window quits.
    cx.on_action(|_: &Quit, cx| {
        let windows = cx.windows();
        if windows.is_empty() {
            cx.quit();
        }
        for window in windows {
            window
                .update(cx, |_, window, cx| {
                    window.dispatch_action(Box::new(CloseWindow), cx)
                })
                .ok();
        }
    });
}

/// Opens a new document window (a tab, by default) and sets it up.
pub fn open_window(
    cx: &mut App,
    setup: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>),
) -> Option<WindowHandle<DocumentView>> {
    let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kami".into()),
                    ..Default::default()
                }),
                tabbing_identifier: Some(TABBING_IDENTIFIER.to_owned()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| DocumentView::new(window, cx)),
        )
        .ok()?;
    window
        .update(cx, |view, window, cx| setup(view, window, cx))
        .ok();
    cx.activate(true);
    Some(window)
}

/// Opens `path`: switches to its window if it's already open, reuses the
/// current window if it's empty, or opens a new tab.
pub fn open_path(path: PathBuf, cx: &mut App) {
    let windows: Vec<WindowHandle<DocumentView>> = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<DocumentView>())
        .collect();

    let already_open = windows.iter().find(|window| {
        window
            .read(cx)
            .ok()
            .and_then(|view| view.document_path())
            .is_some_and(|open| same_file(open, &path))
    });
    if let Some(window) = already_open {
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        return;
    }

    let pristine_active = cx
        .active_window()
        .and_then(|window| window.downcast::<DocumentView>())
        .filter(|window| window.read(cx).is_ok_and(|view| view.is_pristine()));
    match pristine_active {
        Some(window) => {
            window
                .update(cx, |view, window, cx| view.open_path(path, window, cx))
                .ok();
        }
        None => {
            open_window(cx, |view, window, cx| view.open_path(path, window, cx));
        }
    }
}

/// Shows the Open dialog and opens the chosen files.
fn choose_and_open(cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: None,
    });
    cx.spawn(async move |cx| {
        // The outer error means the dialog was dropped, the inner one that
        // the platform couldn't show it; `None` means the user cancelled.
        let Ok(Ok(Some(paths))) = paths.await else {
            return;
        };
        cx.update(|cx| {
            for path in paths {
                open_path(path, cx);
            }
        });
    })
    .detach();
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
