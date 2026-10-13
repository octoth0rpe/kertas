//! Document windows: one document per window.
//!
//! The `layout` setting picks how they're arranged (REQUIREMENTS §8.2):
//! - `Tabs` (the default): native macOS tabs, with Kami's own tab strip
//!   (see `tab_strip.rs`).
//! - `Windows`: separate windows, each offset from the previous one.
//!
//! Kami decides this itself rather than following the macOS "Prefer tabs
//! when opening documents" preference. Changing the setting takes effect
//! when Kami restarts.

use std::path::{Path, PathBuf};

use gpui::{
    App, Bounds, Context, PathPromptOptions, SystemWindowTabController, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, point, prelude::*, px, size,
};

use kami::settings::Layout;

use crate::app_settings;
use crate::document_view::{CloseWindow, DocumentView, New, Open, Quit};
#[cfg(target_os = "macos")]
use crate::native_tabs::{NativeWindow, add_as_tab};

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
    // The frontmost document window, even if another window (such as
    // Settings) is active.
    let previous = cx
        .active_window()
        .into_iter()
        .chain(cx.window_stack().unwrap_or_default())
        .find_map(|window| window.downcast::<DocumentView>());
    let previous_bounds =
        previous.and_then(|window| window.update(cx, |_, window, _| window.bounds()).ok());
    let layout = app_settings::launch_layout(cx);
    let bounds = match (layout, previous_bounds) {
        (Layout::Windows, Some(previous)) => Bounds::new(
            previous.origin + point(px(CASCADE_OFFSET), px(CASCADE_OFFSET)),
            previous.size,
        ),
        // A tab takes its group's frame, so any bounds will do.
        (Layout::Tabs, Some(previous)) => previous,
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
                tabbing_identifier: (layout == Layout::Tabs).then(|| TABBING_IDENTIFIER.to_owned()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| DocumentView::new(window, cx)),
        )
        .ok()?;

    #[cfg(target_os = "macos")]
    crate::native_tabs::disable_automatic_tabbing();
    #[cfg(target_os = "macos")]
    if layout == Layout::Tabs
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
            // GPUI recorded the window's tab group when it was created, before
            // it joined this one; update it so the tab strip is right.
            window
                .update(cx, |_, window, cx| {
                    let id = window.window_handle().window_id();
                    if let Some(tabs) = window.tabbed_windows() {
                        SystemWindowTabController::remove_tab(cx, id);
                        SystemWindowTabController::add_tab(cx, id, tabs);
                    }
                })
                .ok();
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

    // Prefer the active window. Right after launch there may not be one
    // yet, so fall back to any empty window.
    let pristine =
        |window: &WindowHandle<DocumentView>| window.read(cx).is_ok_and(|view| view.is_pristine());
    let pristine_window = cx
        .active_window()
        .and_then(|window| window.downcast::<DocumentView>())
        .filter(pristine)
        .or_else(|| windows.iter().copied().find(pristine));
    match pristine_window {
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

/// The path of a `file://` URL, as sent when files are opened from Finder or
/// the Dock. Other URLs are ignored.
pub fn path_from_file_url(url: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let path = url.strip_prefix("file://")?;
    let path = path.strip_prefix("localhost").unwrap_or(path);
    if !path.starts_with('/') {
        return None;
    }
    // Percent-decode to bytes: paths aren't necessarily UTF-8.
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| path.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            path_from_file_url("file:///Users/me/My%20Notes/caf%C3%A9.md"),
            Some(PathBuf::from("/Users/me/My Notes/café.md"))
        );
        assert_eq!(
            path_from_file_url("file://localhost/tmp/a.md"),
            Some(PathBuf::from("/tmp/a.md"))
        );
        assert_eq!(
            path_from_file_url("file:///tmp/100%"),
            Some(PathBuf::from("/tmp/100%"))
        );
        assert_eq!(path_from_file_url("https://example.com/a.md"), None);
    }
}
