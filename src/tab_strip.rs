//! The tab strip shown when a window holds several documents as tabs.
//!
//! GPUI groups windows into native macOS tabs and tracks the groups in
//! `SystemWindowTabController`, but it hides the native tab bar so apps can
//! draw their own. This is Kertas's.

use gpui::{
    AccessibleAction, AnyElement, AnyWindowHandle, App, MouseButton, Role, SharedString,
    SystemWindowTabController, Window, div, prelude::*, px,
};

use crate::document_window::{CloseWindow, DocumentWindowHandle};
use kertas_editor::Theme;

const STRIP_HEIGHT: f32 = 30.;
const MAX_TAB_WIDTH: f32 = 240.;

/// The tab strip for `window`, if it's in a tab group with other windows.
/// `self_dirty` is whether this window's own document has unsaved changes
/// (its window is being rendered, so it can't be read here).
pub fn render_tab_strip(
    window: &Window,
    cx: &App,
    theme: Theme,
    self_dirty: bool,
) -> Option<AnyElement> {
    let id = window.window_handle().window_id();
    let tabs = cx.try_global::<SystemWindowTabController>()?.tabs(id)?;
    if tabs.len() < 2 {
        return None;
    }

    let count = tabs.len();
    let tabs = tabs.iter().enumerate().map(|(index, tab)| {
        let active = tab.id == id;
        let dirty = if active {
            self_dirty
        } else {
            DocumentWindowHandle::downcast(tab.handle)
                .and_then(|handle| handle.read(cx).ok())
                .is_some_and(|view| view.is_dirty())
        };
        let handle = tab.handle;
        let title = tab.title.clone();
        let close_name = if dirty {
            format!("Close {title}, which has unsaved changes")
        } else {
            format!("Close {title}")
        };
        div()
            .id(("tab", index))
            .role(Role::Tab)
            .aria_label(title.clone())
            .aria_selected(active)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(count)
            .when(dirty, |this| this.aria_description("Edited"))
            .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                activate(handle, cx)
            })
            .flex_1()
            .min_w(px(0.))
            .max_w(px(MAX_TAB_WIDTH))
            .h_full()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_r_1()
            .border_color(theme.rule)
            .text_size(px(12.))
            .line_height(px(STRIP_HEIGHT))
            .when(active, |this| {
                this.bg(theme.background).text_color(theme.text)
            })
            .when(!active, |this| {
                this.text_color(theme.muted_text)
                    .hover(|style| style.text_color(theme.text))
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                if !active {
                    activate(handle, cx);
                }
            })
            .child(div().flex_1().min_w(px(0.)).truncate().child(title))
            .child(
                div()
                    .id(("close-tab", index))
                    .role(Role::Button)
                    .aria_label(SharedString::from(close_name))
                    .on_a11y_action(AccessibleAction::Click, move |_, _, cx| close(handle, cx))
                    .flex_none()
                    .w(px(14.))
                    .text_center()
                    .rounded_sm()
                    .hover(|style| style.bg(theme.rule))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        close(handle, cx);
                        cx.stop_propagation();
                    })
                    .child(if dirty { "●" } else { "×" }),
            )
    });

    Some(
        div()
            .id("tabs")
            .role(Role::TabList)
            .aria_label("Documents")
            .flex_none()
            .flex()
            .h(px(STRIP_HEIGHT))
            .w_full()
            .bg(theme.table_header_background)
            .border_b_1()
            .border_color(theme.rule)
            .children(tabs)
            .into_any_element(),
    )
}

/// Brings a tab's window to the front. Deferred, since the event is
/// delivered while this window is locked.
fn activate(handle: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    });
}

/// Closes a tab, asking about unsaved changes first.
fn close(handle: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| {
        handle
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(CloseWindow), cx)
            })
            .ok();
    });
}

/// Switches to the tab at `index` (0-based) in `window`'s tab group, for
/// ⌘1 to ⌘9. Does nothing if there's no such tab (as in the
/// windows layout, where each window is on its own).
pub fn select_tab(index: usize, window: &Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let Some(handle) = cx
        .try_global::<SystemWindowTabController>()
        .and_then(|controller| controller.tabs(id))
        .and_then(|tabs| tabs.get(index))
        .map(|tab| tab.handle)
    else {
        return;
    };
    if handle.window_id() != id {
        // Another window, so it can be updated while this one handles the
        // key.
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}
