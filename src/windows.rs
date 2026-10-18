//! Document windows: one document per window.
//!
//! The `layout` setting picks how they're arranged (REQUIREMENTS §8.2):
//! - `Tabs` (the default): native macOS tabs, with Kertas's own tab strip
//!   (see `tab_strip.rs`).
//! - `Windows`: separate windows, each offset from the previous one.
//!
//! Kertas decides this itself rather than following the macOS "Prefer tabs
//! when opening documents" preference. Changing the setting takes effect
//! when Kertas restarts.

use std::path::{Path, PathBuf};

use gpui::{
    App, Bounds, Context, PathPromptOptions, SystemWindowTabController, TitlebarOptions, Window,
    WindowBounds, WindowOptions, point, prelude::*, px, size,
};

use kertas_core::settings::Layout;
use kertas_core::titles;

use crate::app_settings::{self, AppSettings};
use crate::document_view::DocumentView;
use crate::document_window::{CloseWindow, DocumentWindow, DocumentWindowHandle, New, Open, Quit};
use crate::editor_config::EditorConfig;
#[cfg(target_os = "macos")]
use crate::native_tabs::{add_as_tab, ns_window};
use crate::text_style::TextSettings;
use crate::zoom::{self, Zoom};

/// Groups document windows into native tabs.
const TABBING_IDENTIFIER: &str = "kertas-documents";

/// How far each new window is offset from the previous one in the
/// `Windows` layout.
const CASCADE_OFFSET: f32 = 24.;

/// Registers the app-wide document actions, and keeps document views'
/// configuration up to date.
///
/// The handlers defer their work: an action is delivered while its window
/// is locked for the dispatch, so updating that window from the handler
/// itself would silently fail.
pub fn register(cx: &mut App) {
    cx.observe_global::<AppSettings>(update_configs).detach();
    cx.observe_global::<Zoom>(update_configs).detach();
    cx.on_action(|_: &New, cx| {
        cx.defer(|cx| {
            // The startup message isn't really a tab, so New replaces it.
            let blank = frontmost_document_window(cx)
                .filter(|window| window.read(cx).is_ok_and(|view| view.is_blank()));
            match blank {
                Some(window) => {
                    window
                        .update(cx, |view, window, cx| {
                            view.start_untitled(window, cx);
                            window.activate_window();
                        })
                        .ok();
                }
                None => {
                    open_window(cx, |view, window, cx| view.start_untitled(window, cx));
                }
            }
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

/// What document views are told: the settings, at the current zoom.
fn editor_config(cx: &App) -> EditorConfig {
    let settings = app_settings::get(cx);
    EditorConfig {
        text: TextSettings::from(&settings),
        theme: app_settings::theme(cx),
        appearance: settings.appearance,
        font_size: zoom::size(cx),
        default_font_size: zoom::default_size(cx),
        autosave: settings.autosave,
        scrollbar: settings.scrollbar,
        editing_hint: settings.editing_hint,
    }
}

/// Gives every document view the configuration, after the settings or the
/// zoom change.
fn update_configs(cx: &mut App) {
    let config = editor_config(cx);
    for window in cx.windows() {
        if let Some(window) = DocumentWindowHandle::downcast(window) {
            window
                .update(cx, |view, window, cx| {
                    view.set_config(config.clone(), window, cx)
                })
                .ok();
        }
    }
}

/// The frontmost document window, even if another window (such as
/// Settings) is active.
fn frontmost_document_window(cx: &App) -> Option<DocumentWindowHandle> {
    cx.active_window()
        .into_iter()
        .chain(cx.window_stack().unwrap_or_default())
        .find_map(DocumentWindowHandle::downcast)
}

/// Gives every document window its title, adding the folder to files that
/// share a name with another open file (#95). Call it when a window opens
/// a file, saves to a new one, or closes.
pub fn refresh_titles(cx: &mut App) {
    let windows: Vec<DocumentWindowHandle> = cx
        .windows()
        .into_iter()
        .filter_map(DocumentWindowHandle::downcast)
        .collect();
    let paths: Vec<Option<PathBuf>> = windows
        .iter()
        .map(|window| {
            window
                .read(cx)
                .ok()
                .and_then(|view| view.document_path().map(Path::to_owned))
        })
        .collect();
    let paths: Vec<Option<&Path>> = paths.iter().map(Option::as_deref).collect();
    let hints = titles::folder_hints(&paths);
    for (window, hint) in windows.into_iter().zip(hints) {
        window
            .0
            .update(cx, |this, window, cx| {
                this.set_title_hint(hint, window, cx);
                // Its tab strip shows the other tabs' titles.
                cx.notify();
            })
            .ok();
    }
}

/// Opens a new document window (a tab, by default) and sets it up.
pub fn open_window(
    cx: &mut App,
    setup: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>),
) -> Option<DocumentWindowHandle> {
    let previous = frontmost_document_window(cx);
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
                    title: Some("Kertas".into()),
                    ..Default::default()
                }),
                tabbing_identifier: (layout == Layout::Tabs).then(|| TABBING_IDENTIFIER.to_owned()),
                ..Default::default()
            },
            |window, cx| {
                let config = editor_config(cx);
                let view = cx.new(|cx| DocumentView::new(config, window, cx));
                cx.new(|cx| DocumentWindow::new(view, window, cx))
            },
        )
        .ok()?;
    let window = DocumentWindowHandle(window);

    #[cfg(target_os = "macos")]
    crate::native_tabs::disable_automatic_tabbing();
    #[cfg(target_os = "macos")]
    if layout == Layout::Tabs
        && let Some(previous) = previous
    {
        let host = previous
            .update(cx, |_, window, _| ns_window(window))
            .ok()
            .flatten();
        let tab = window
            .update(cx, |_, window, _| ns_window(window))
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
    open_path_at(path, None, cx);
}

/// Opens `path` as [`open_path`] does, then scrolls to the heading named
/// `anchor`, if there is one (`other.md#setup`).
pub fn open_path_at(path: PathBuf, anchor: Option<String>, cx: &mut App) {
    let go_to_anchor = move |view: &mut DocumentView, cx: &mut Context<DocumentView>| {
        if let Some(anchor) = &anchor
            && view.document_path().is_some()
        {
            view.go_to_anchor(anchor, cx);
        }
    };
    let windows: Vec<DocumentWindowHandle> = cx
        .windows()
        .into_iter()
        .filter_map(DocumentWindowHandle::downcast)
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
            .update(cx, |view, window, cx| {
                window.activate_window();
                go_to_anchor(view, cx);
            })
            .ok();
        return;
    }

    // Prefer the active window. Right after launch there may not be one
    // yet, so fall back to any empty window.
    let pristine =
        |window: &DocumentWindowHandle| window.read(cx).is_ok_and(|view| view.is_pristine());
    let pristine_window = cx
        .active_window()
        .and_then(DocumentWindowHandle::downcast)
        .filter(pristine)
        .or_else(|| windows.iter().copied().find(pristine));
    match pristine_window {
        Some(window) => {
            window
                .update(cx, |view, window, cx| {
                    view.open_path(path, window, cx);
                    go_to_anchor(view, cx);
                })
                .ok();
        }
        None => {
            open_window(cx, |view, window, cx| {
                view.open_path(path, window, cx);
                go_to_anchor(view, cx);
            });
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

/// What a URL sent to Kertas asks for: from Finder and the Dock (`file://`),
/// or from the `kt` command (`kertas://`).
#[derive(Debug, PartialEq, Eq)]
pub enum UrlRequest {
    /// Open this file, or start a new document saved there if it doesn't
    /// exist yet.
    Open(PathBuf),
    /// Just bring Kertas to the front.
    Activate,
}

pub fn parse_url(url: &str) -> Option<UrlRequest> {
    if url == "kertas://activate" {
        return Some(UrlRequest::Activate);
    }
    let path = url
        .strip_prefix("kertas://open?path=")
        .or_else(|| url.strip_prefix("file://localhost"))
        .or_else(|| url.strip_prefix("file://"))?;
    path.starts_with('/')
        .then(|| UrlRequest::Open(kertas_core::links::percent_decode(path)))
}

/// Handles a URL sent to Kertas.
pub fn handle_url(url: &str, cx: &mut App) {
    match parse_url(url) {
        Some(UrlRequest::Open(path)) => open_path(path, cx),
        Some(UrlRequest::Activate) => {
            cx.activate(true);
            if cx.windows().is_empty() {
                open_window(cx, |view, window, cx| view.start_untitled(window, cx));
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(path: &str) -> Option<UrlRequest> {
        Some(UrlRequest::Open(PathBuf::from(path)))
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            parse_url("file:///Users/me/My%20Notes/caf%C3%A9.md"),
            open("/Users/me/My Notes/café.md")
        );
        assert_eq!(parse_url("file://localhost/tmp/a.md"), open("/tmp/a.md"));
        assert_eq!(parse_url("file:///tmp/100%"), open("/tmp/100%"));
        assert_eq!(parse_url("https://example.com/a.md"), None);
    }

    #[test]
    fn kertas_urls_from_the_command_line() {
        assert_eq!(
            parse_url("kertas://open?path=/tmp/My%20Notes/new.md"),
            open("/tmp/My Notes/new.md")
        );
        assert_eq!(parse_url("kertas://activate"), Some(UrlRequest::Activate));
        assert_eq!(parse_url("kertas://open?path=relative.md"), None);
        assert_eq!(parse_url("kertas://other"), None);
    }
}
