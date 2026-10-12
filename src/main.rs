use gpui::{
    App, Bounds, Context, FocusHandle, KeyBinding, Menu, MenuItem, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, size,
};
use gpui_platform::application;

actions!(kami, [Quit, CloseWindow]);

struct KamiWindow {
    focus_handle: FocusHandle,
}

impl Render for KamiWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
        ]);
        cx.set_menus([Menu::new("Kami").items([MenuItem::action("Quit Kami", Quit)])]);

        // Kami is a single-window app for now, so closing the window quits.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kami".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus_handle = cx.focus_handle();
                    focus_handle.focus(window, cx);
                    KamiWindow { focus_handle }
                })
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
