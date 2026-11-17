//! A title bar the app draws itself, so it can hold the window's tabs, as
//! iTerm2's compact tabs do, rather than the system's title bar with a
//! strip of tabs below it.
//!
//! On macOS the app opens its window with [`window_options`]: the
//! system's title bar is see-through, and the traffic lights sit on the
//! app's own. On Linux the app draws the title bar where the compositor
//! asks it to draw the window's frame (client-side decorations, as on
//! GNOME); elsewhere the system draws it, with the window's title, and
//! the tabs go in a strip below it. [`is_drawn_by_app`] says which.
//!
//! The bar ([`bar`]) is [`HEIGHT`] tall. Its empty parts are
//! [`drag_area`]s, which move the window when dragged and zoom it when
//! double-clicked, as the system's title bar does. On macOS, the bar
//! leaves [`traffic_light_inset`] free at its start for the traffic
//! lights, unless something else, such as a sidebar, is drawn there.

use gpui::{
    Div, ElementId, MouseButton, Pixels, Point, SharedString, Stateful, TitlebarOptions, Window,
    WindowOptions, div, point, prelude::*, px,
};

use crate::Theme;

/// How tall the title bar is.
pub const HEIGHT: f32 = 34.;

/// How wide the traffic lights are on macOS, with the space around them.
/// macOS 26's are a little wider than earlier versions', so this leaves
/// room for either.
const TRAFFIC_LIGHTS_WIDTH: f32 = 78.;

/// The traffic lights' size, to center them in the bar.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;

/// `options` for a window whose title bar the app draws: on macOS, the
/// system's is see-through, the traffic lights are centered in the
/// app's, and AppKit leaves clicks there to the app, so a tab can be
/// dragged without dragging the window. `title` is still the window's
/// title, as the Window menu and Mission Control show it.
pub fn window_options(title: impl Into<SharedString>, options: WindowOptions) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(traffic_light_position()),
        }),
        app_owns_titlebar_drag: true,
        ..options
    }
}

fn traffic_light_position() -> Point<Pixels> {
    let inset = (HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.;
    point(px(inset + 2.), px(inset))
}

/// Whether the app draws `window`'s title bar: always on macOS, with
/// [`window_options`], and on Linux where the compositor leaves the
/// window's frame to the app. Otherwise the system draws it, and the app
/// shows its tabs in a strip below.
pub fn is_drawn_by_app(window: &Window) -> bool {
    cfg!(target_os = "macos")
        || matches!(
            window.window_decorations(),
            gpui::Decorations::Client { .. }
        )
}

/// The width at the start of the title bar that the traffic lights take,
/// on macOS outside full screen; otherwise none.
pub fn traffic_light_inset(window: &Window) -> Pixels {
    if cfg!(target_os = "macos") && !window.is_fullscreen() {
        px(TRAFFIC_LIGHTS_WIDTH)
    } else {
        px(0.)
    }
}

/// The title bar, `theme`'s colors, for the app to fill: with
/// [`drag_area`]s, tabs ([`Tabs::title_bar_tabs`](crate::tabs::Tabs::title_bar_tabs))
/// and, on Linux, the window's buttons. Its children are as tall as it
/// is, and each draws its own share of the line along its bottom, so the
/// active tab can open into the content below.
pub fn bar(theme: Theme) -> Div {
    div()
        .flex_none()
        .h(px(HEIGHT))
        .w_full()
        .flex()
        .items_stretch()
        .overflow_hidden()
        .bg(theme.table_header_background)
        .text_color(theme.text)
}

/// A part of the title bar that moves the window when dragged, and zooms
/// it when double-clicked, as the system's title bar does (on macOS, as
/// the user's Dock setting says). On Linux, a secondary click shows the
/// window menu. In the bar, it draws the bar's line along its bottom with
/// [`bottom_line`].
pub fn drag_area(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .h_full()
        .on_mouse_down(MouseButton::Left, |event, window, cx| {
            cx.stop_propagation();
            if event.click_count == 2 {
                if cfg!(target_os = "macos") {
                    window.titlebar_double_click();
                } else {
                    window.zoom_window();
                }
            } else {
                window.start_window_move();
            }
        })
        .when(cfg!(target_os = "linux"), |this| {
            this.on_mouse_down(MouseButton::Right, |event, window, cx| {
                cx.stop_propagation();
                window.show_window_menu(event.position);
            })
        })
}

/// The line along the bottom of the bar, which each of its children
/// draws its share of.
pub fn bottom_line<E: Styled>(element: E, theme: Theme) -> E {
    element.border_b_1().border_color(theme.rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_lights_are_centered_in_the_bar() {
        let position = traffic_light_position();
        assert_eq!(position.y * 2. + px(TRAFFIC_LIGHT_SIZE), px(HEIGHT));
    }

    #[test]
    fn the_window_keeps_its_title_and_other_options() {
        let options = window_options(
            "Filer",
            WindowOptions {
                is_resizable: false,
                ..Default::default()
            },
        );
        let titlebar = options.titlebar.unwrap();
        assert_eq!(titlebar.title.as_deref(), Some("Filer"));
        assert!(titlebar.appears_transparent);
        assert!(options.app_owns_titlebar_drag);
        assert!(!options.is_resizable);
    }
}
