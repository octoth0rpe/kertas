mod document_view;
#[cfg(target_os = "macos")]
mod native_tabs;
mod rendered_block;
mod revealed_block;
#[cfg(debug_assertions)]
mod smoke;
mod theme;
mod windows;

use std::path::PathBuf;

use document_view::{CloseWindow, New, Open, Quit, Save, SaveAs};
use gpui::{App, Menu, MenuItem};
use gpui_platform::application;

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    application().run(move |cx: &mut App| {
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
