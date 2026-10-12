mod document_view;
mod rendered_block;
mod revealed_block;
mod theme;

use std::path::PathBuf;

use document_view::{CloseWindow, DocumentView, Open};
use gpui::{
    App, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions, actions,
    prelude::*, px, size,
};
use gpui_platform::application;

actions!(kami, [Quit]);

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    application().run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
            KeyBinding::new("cmd-o", Open, None),
        ]);
        cx.bind_keys(document_view::key_bindings());
        cx.set_menus([
            Menu::new("Kami").items([MenuItem::action("Quit Kami", Quit)]),
            Menu::new("File").items([
                MenuItem::action("Open…", Open),
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
        cx.activate(true);
    });
}
