//! A window that holds one document view and nothing else.
//!
//!     cargo run -p kertas-editor --example embed -- README.md
//!
//! With no file, it starts an untitled document. ⌘S saves it.

use std::path::PathBuf;

use kertas_editor::gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use kertas_editor::gpui_platform::application;
use kertas_editor::{Assets, DocumentView, EditorConfig, key_bindings};

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    application().with_assets(Assets).run(move |cx: &mut App| {
        cx.bind_keys(key_bindings());
        let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| DocumentView::new(EditorConfig::default(), window, cx));
            view.update(cx, |view, cx| match path {
                Some(path) => view.open_path(path, window, cx),
                None => view.start_untitled(window, cx),
            });
            view
        })
        .expect("couldn't open a window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}
