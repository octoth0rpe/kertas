//! Document windows: one document per window.
//!
//! [`DOCUMENT_LAYOUT`] picks how they're arranged (REQUIREMENTS §8.2):
//! - `Tabs` (the default): native macOS tabs. macOS provides the tab bar,
//!   dragging tabs out into windows, and Window > Merge All Windows.
//! - `Windows`: separate windows, each offset from the previous one.
//!
//! Kami decides this itself rather than following the macOS "Prefer tabs
//! when opening documents" preference. The setting moves to the config
//! file (§8) later.

use std::path::{Path, PathBuf};

use gpui::{
    App, Bounds, Context, PathPromptOptions, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions, point, prelude::*, px, size,
};

use crate::document_view::{CloseWindow, DocumentView, New, Open, Quit};
#[cfg(target_os = "macos")]
use crate::native_tabs::{NativeWindow, add_as_tab};

/// How document windows are arranged (§8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "`Windows` is selectable once the config file exists"
)]
pub enum DocumentLayout {
    Tabs,
    Windows,
}

pub const DOCUMENT_LAYOUT: DocumentLayout = DocumentLayout::Tabs;

/// Groups document windows into native tabs.
const TABBING_IDENTIFIER: &str = "kami-documents";

/// How far each new window is offset from the previous one in the
/// `Windows` layout.
const CASCADE_OFFSET: f32 = 24.;

/// Registers the app-wide document actions.
///
/// The handlers defer their work: an action is delivered while its window
/// is locked for the dispatch, so updating that window from the handler
/// itself would silently fail.
pub fn register(cx: &mut App) {
    cx.on_action(|_: &New, cx| {
        cx.defer(|cx| {
            open_window(cx, |view, window, cx| view.start_untitled(window, cx));
        });
    });
    cx.on_action(|_: &Open, cx| cx.defer(choose_and_open));
    // Quitting closes each window, which asks about unsaved changes;
    // closing the last window quits.
    cx.on_action(|_: &Quit, cx| {
        cx.defer(|cx| {
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
    });
}

/// Opens a new document window (a tab, by default) and sets it up.
pub fn open_window(
    cx: &mut App,
    setup: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>),
) -> Option<WindowHandle<DocumentView>> {
    let previous = cx
        .active_window()
        .and_then(|window| window.downcast::<DocumentView>());
    let previous_bounds =
        previous.and_then(|window| window.update(cx, |_, window, _| window.bounds()).ok());
    let bounds = match (DOCUMENT_LAYOUT, previous_bounds) {
        (DocumentLayout::Windows, Some(previous)) => Bounds::new(
            previous.origin + point(px(CASCADE_OFFSET), px(CASCADE_OFFSET)),
            previous.size,
        ),
        // A tab takes its group's frame, so any bounds will do.
        (DocumentLayout::Tabs, Some(previous)) => previous,
        (_, None) => Bounds::centered(None, size(px(900.), px(700.)), cx),
    };
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kami".into()),
                    ..Default::default()
                }),
                tabbing_identifier: (DOCUMENT_LAYOUT == DocumentLayout::Tabs)
                    .then(|| TABBING_IDENTIFIER.to_owned()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| DocumentView::new(window, cx)),
        )
        .ok()?;

    #[cfg(target_os = "macos")]
    if DOCUMENT_LAYOUT == DocumentLayout::Tabs
        && let Some(previous) = previous
    {
        let host = previous
            .update(cx, |_, window, _| NativeWindow::of(window))
            .ok()
            .flatten();
        let tab = window
            .update(cx, |_, window, _| NativeWindow::of(window))
            .ok()
            .flatten();
        if let (Some(host), Some(tab)) = (host, tab) {
            add_as_tab(&host, &tab);
        }
    }

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
