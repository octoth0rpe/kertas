//! The text size for this session.
//!
//! Zooming changes the size in every document. It isn't saved: the app
//! starts at the `font_size` setting, and Reset Zoom returns to it.
//! Changing the setting also changes the current size.

use gpui::prelude::FluentBuilder;
use gpui::{App, Global, InteractiveElement, KeyBinding, actions};
use kertas_core::settings::FONT_SIZES;

use crate::app_settings::{self, AppSettings};

actions!(kertas, [ZoomIn, ZoomOut, ResetZoom]);

/// The zoom keys: ⌘+ (or ⌘=), ⌘- and ⌘0.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ResetZoom, None),
    ]
}

/// `element` handling the zoom actions. Each is handled only while
/// possible, so its menu item is disabled at the limits.
pub fn handle_actions<E: InteractiveElement + FluentBuilder>(element: E, cx: &App) -> E {
    let size = size(cx);
    let zoomed = size != default_size(cx);
    element
        .when_some(larger(size), |this, size| {
            this.on_action(move |_: &ZoomIn, _, cx| set(size, cx))
        })
        .when_some(smaller(size), |this, size| {
            this.on_action(move |_: &ZoomOut, _, cx| set(size, cx))
        })
        .when(zoomed, |this| {
            this.on_action(|_: &ResetZoom, _, cx| reset(cx))
        })
}

/// The sizes Zoom In and Zoom Out step through, in pixels.
const STEPS: &[u32] = &[
    10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 26, 28, 30, 32,
];

/// The current text size, and the setting it started from.
pub struct Zoom {
    pub size: u32,
    default: u32,
}

impl Global for Zoom {}

/// Starts at the `font_size` setting, and follows changes to it.
pub(crate) fn init(cx: &mut App) {
    let default = app_settings::get(cx).font_size;
    cx.set_global(Zoom {
        size: default,
        default,
    });
    cx.observe_global::<AppSettings>(|cx| {
        let default = app_settings::get(cx).font_size;
        if default != cx.global::<Zoom>().default {
            cx.set_global(Zoom {
                size: default,
                default,
            });
        }
    })
    .detach();
}

/// The current text size, in pixels.
pub fn size(cx: &App) -> u32 {
    cx.try_global::<Zoom>()
        .map_or(app_settings::get(cx).font_size, |zoom| zoom.size)
}

/// The size zooming started from: the `font_size` setting.
pub fn default_size(cx: &App) -> u32 {
    cx.try_global::<Zoom>()
        .map_or(app_settings::get(cx).font_size, |zoom| zoom.default)
}

/// The next size up from `size`, if there is one.
pub fn larger(size: u32) -> Option<u32> {
    STEPS.iter().copied().find(|&step| step > size)
}

/// The next size down from `size`, if there is one.
pub fn smaller(size: u32) -> Option<u32> {
    STEPS.iter().copied().rev().find(|&step| step < size)
}

/// Sets the size for this session.
pub fn set(size: u32, cx: &mut App) {
    let size = size.clamp(*FONT_SIZES.start(), *FONT_SIZES.end());
    let default = cx.global::<Zoom>().default;
    cx.set_global(Zoom { size, default });
}

/// Returns to the `font_size` setting.
pub fn reset(cx: &mut App) {
    let default = cx.global::<Zoom>().default;
    set(default, cx);
}
