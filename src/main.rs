mod document_view;
mod rendered_block;
mod revealed_block;
#[cfg(debug_assertions)]
mod smoke;
mod theme;

use std::path::PathBuf;

use document_view::{CloseWindow, DocumentView, New, Open, Quit, Save, SaveAs};
use gpui::{
    App, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size,
};
use gpui_platform::application;

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    application().run(move |cx: &mut App| {
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
                MenuItem::action("Close Window", CloseWindow),
            ]),
        ]);

        // Kami is a single-window app for now, so closing the window quits.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Kami".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| DocumentView::new(window, cx)),
            )
            .expect("failed to open window");

        if let Some(path) = path.clone() {
            window
                .update(cx, |view, window, cx| view.open_path(path, window, cx))
                .ok();
        }
        #[cfg(debug_assertions)]
        smoke::start_if_requested(window, cx);
        cx.activate(true);
    });
}
