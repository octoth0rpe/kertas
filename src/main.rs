mod app_settings;
mod document_view;
#[cfg(target_os = "macos")]
mod native_tabs;
mod rendered_block;
mod revealed_block;
#[cfg(debug_assertions)]
mod smoke;
mod tab_strip;
mod theme;
mod windows;

use std::path::PathBuf;

use document_view::{CloseWindow, New, Open, Quit, Save, SaveAs};
use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{App, Menu, MenuItem};
use gpui_platform::application;

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    // Files opened from Finder or the Dock arrive as URLs, outside of any
    // app context, so pass them to a task that opens them.
    let (opened_urls, mut urls_to_open) = mpsc::unbounded::<Vec<String>>();
    let app = application();
    app.on_open_urls(move |urls| {
        opened_urls.unbounded_send(urls).ok();
    });

    app.run(move |cx: &mut App| {
        app_settings::init(cx);
        windows::register(cx);
        cx.bind_keys(document_view::key_bindings());
        cx.set_menus([
            Menu::new("Kami").items([MenuItem::action("Quit Kami", Quit)]),
            Menu::new("File").items([
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::separator(),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::separator(),
                MenuItem::action("Close", CloseWindow),
            ]),
        ]);

        // Closing the last window quits.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            while let Some(urls) = urls_to_open.next().await {
                cx.update(|cx| {
                    for path in urls
                        .iter()
                        .filter_map(|url| windows::path_from_file_url(url))
                    {
                        windows::open_path(path, cx);
                    }
                });
            }
        })
        .detach();

        let window = windows::open_window(cx, |view, window, cx| {
            if let Some(path) = path.clone() {
                view.open_path(path, window, cx);
            }
        });
        #[cfg(debug_assertions)]
        if let Some(window) = window {
            smoke::start_if_requested(window, cx);
        }
        #[cfg(not(debug_assertions))]
        let _ = window;
    });
}
