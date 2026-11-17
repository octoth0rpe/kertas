//! The parts of Kertas's windows that other apps share, so their windows
//! look and work like Kertas's: [`tabs`], a single-line [`text_field`], a
//! [`context_menu`], and the [`Theme`] they're drawn in.
//!
//! [`tabs::Tabs`] holds any view that implements [`tabs::TabItem`], one
//! shown at a time; [`tabs::tab_strip`] and [`tabs::tab`] draw an app's
//! own strips, such as one switching between panels, the same way.
//!
//! A [`Theme`] is a color theme's light or dark colors
//! ([`kertas_core::themes`]). A view is given one, or asks
//! [`theme::window_theme`] for the app's, which an app chooses with
//! [`theme::set_window_theme`].
//!
//! This crate re-exports [`gpui`] and [`gpui_platform`], so an app uses
//! the same GPUI as these views, and [`kertas_core`], whose themes and
//! settings a [`Theme`] is made from.

pub mod context_menu;
pub mod tabs;
pub mod text_field;
pub mod theme;

pub use gpui;
pub use gpui_platform;
pub use kertas_core;

pub use theme::Theme;

use std::time::Duration;

/// How long the caret stays on, and then off, as it blinks, in text
/// fields and in Kertas's document views alike.
pub const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);
