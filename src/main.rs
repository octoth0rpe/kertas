mod app_settings;
mod assets;
mod builtin;
#[cfg(target_os = "macos")]
mod cli_tool;
#[cfg(target_os = "macos")]
mod convert;
#[cfg(target_os = "macos")]
mod default_app;
#[cfg(target_os = "macos")]
mod dock_icon;
mod document_view;
mod document_window;
mod editor_config;
#[cfg(target_os = "macos")]
mod file_watch;
mod menus;
#[cfg(target_os = "macos")]
mod native_tabs;
#[cfg(target_os = "macos")]
mod print;
mod rendered_block;
mod revealed_block;
mod settings_window;
#[cfg(debug_assertions)]
mod smoke;
#[cfg(target_os = "macos")]
mod system_actions;
mod tab_strip;
mod text_style;
mod theme;
#[cfg(target_os = "macos")]
mod web_images;
mod windows;
mod zoom;

use std::path::PathBuf;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::App;
use gpui_platform::application;

fn main() -> std::process::ExitCode {
    #[cfg(target_os = "macos")]
    if let Some(code) = convert::run_if_requested() {
        return code;
    }
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    // Files opened from Finder, the Dock or the `kt` command arrive as
    // URLs, outside of any app context, so pass them to a task that opens
    // them.
    let (opened_urls, mut urls_to_open) = mpsc::unbounded::<Vec<String>>();
    let app = application().with_assets(assets::Assets);
    app.on_open_urls(move |urls| {
        opened_urls.unbounded_send(urls).ok();
    });

    app.run(move |cx: &mut App| {
        app_settings::init(cx);
        zoom::init(cx);
        #[cfg(target_os = "macos")]
        web_images::init(cx);
        #[cfg(target_os = "macos")]
        dock_icon::init();
        windows::register(cx);
        cx.bind_keys(document_view::key_bindings());
        cx.bind_keys(document_window::key_bindings());
        cx.bind_keys(settings_window::key_bindings());
        #[cfg(target_os = "macos")]
        system_actions::init(cx);
        menus::init(cx);

        // Closing the last window quits.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            } else {
                // A file that shared its name with the closed one may not
                // need its folder any more.
                cx.defer(windows::refresh_titles);
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            while let Some(urls) = urls_to_open.next().await {
                cx.update(|cx| {
                    for url in &urls {
                        windows::handle_url(url, cx);
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
    std::process::ExitCode::SUCCESS
}
