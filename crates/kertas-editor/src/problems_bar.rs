//! The bar at the bottom of a window that reports problems in a file the
//! app reads, such as the settings file, until the user closes it.

use gpui::{App, ClickEvent, Div, Role, SharedString, Stateful, Window, div, prelude::*, px};

use crate::{Theme, bar_button};

/// A bar showing `message`, with a × button, called `dismiss_name` for
/// VoiceOver, that runs `on_dismiss`. `id` names the bar, and the button
/// is `dismiss-{id}`.
pub fn problems_bar(
    theme: Theme,
    id: &'static str,
    message: impl Into<SharedString>,
    dismiss_name: impl Into<SharedString>,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let message = message.into();
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(message.clone())
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(6.))
        .border_t_1()
        .border_color(theme.rule)
        .bg(theme.table_header_background)
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(theme.muted_text)
        .child(div().flex_1().child(message))
        .child(
            bar_button(theme, format!("dismiss-{id}"), "×", dismiss_name)
                .px_1()
                .py_0()
                .border_color(gpui::transparent_black())
                .bg(gpui::transparent_black())
                .on_click(on_dismiss),
        )
}
