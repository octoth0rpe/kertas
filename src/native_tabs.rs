//! Grouping document windows into native macOS tabs.
//!
//! GPUI only tabs new windows when the macOS "Prefer tabs when opening
//! documents" preference allows it (by default, only in full screen). Kami
//! has its own tabs/windows setting (REQUIREMENTS §8.2), so it adds the new
//! window as a tab itself, and turns automatic tabbing off. Otherwise macOS
//! adds "Show Tab Bar" and "Show All Tabs" to the View menu, which do
//! nothing useful with Kami's own tab strip.

use std::ptr::NonNull;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSView, NSWindow, NSWindowOrderingMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// A native window, captured from a GPUI window.
pub struct NativeWindow(NonNull<std::ffi::c_void>);

impl NativeWindow {
    /// The native window behind a GPUI window.
    pub fn of(window: &impl HasWindowHandle) -> Option<Self> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::AppKit(handle) => Some(Self(handle.ns_view)),
            _ => None,
        }
    }
}

/// Turns off macOS automatic window tabbing. GPUI turns it on whenever it
/// opens a window with a tabbing identifier, so call this after each one.
pub fn disable_automatic_tabbing() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
    }
}

/// Whether macOS automatic window tabbing is on, for the smoke test.
#[cfg(debug_assertions)]
pub fn automatic_tabbing() -> bool {
    MainThreadMarker::new().is_some_and(NSWindow::allowsAutomaticWindowTabbing)
}

impl NativeWindow {
    /// The `NSWindow`, if the view is in one.
    pub fn ns_window(&self) -> Option<objc2::rc::Retained<NSWindow>> {
        // SAFETY: the pointer is the content view of a live GPUI window,
        // captured on the main thread just before this call.
        let view = unsafe { self.0.cast::<NSView>().as_ref() };
        view.window()
    }
}

/// How many times [`add_as_tab`] found both windows, for the smoke test.
#[cfg(debug_assertions)]
pub static TABBING_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Adds `tab` as a tab of `host`'s tab group, if it isn't in it already.
pub fn add_as_tab(host: &NativeWindow, tab: &NativeWindow) {
    // SAFETY: both pointers are the content views of live GPUI windows,
    // captured on the main thread just before this call.
    let (host_view, tab_view) = unsafe {
        (
            host.0.cast::<NSView>().as_ref(),
            tab.0.cast::<NSView>().as_ref(),
        )
    };
    let (Some(host_window), Some(tab_window)) = (host_view.window(), tab_view.window()) else {
        return;
    };
    if host_window == tab_window {
        return;
    }
    #[cfg(debug_assertions)]
    TABBING_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(group) = host_window.tabbedWindows()
        && group.iter().any(|window| window == tab_window)
    {
        return;
    }
    host_window.addTabbedWindow_ordered(&tab_window, NSWindowOrderingMode::Above);
    tab_window.makeKeyAndOrderFront(None);
}

/// How many windows are in `window`'s tab group (1 if it isn't tabbed).
#[cfg(debug_assertions)]
pub fn tab_group_size(window: &NativeWindow) -> usize {
    // SAFETY: the pointer is the content view of a live GPUI window,
    // captured on the main thread just before this call.
    let view = unsafe { window.0.cast::<NSView>().as_ref() };
    view.window()
        .and_then(|window| window.tabbedWindows())
        .map_or(1, |group| group.count())
}

/// The position of the selected tab in `window`'s tab group.
#[cfg(debug_assertions)]
pub fn selected_tab(window: &NativeWindow) -> Option<usize> {
    // SAFETY: as in `tab_group_size`.
    let view = unsafe { window.0.cast::<NSView>().as_ref() };
    let group = view.window()?.tabGroup()?;
    let selected = group.selectedWindow()?;
    group.windows().iter().position(|window| window == selected)
}

/// Whether `window`'s native tab bar is showing.
#[cfg(debug_assertions)]
pub fn tab_bar_visible(window: &NativeWindow) -> bool {
    // SAFETY: as in `tab_group_size`.
    let view = unsafe { window.0.cast::<NSView>().as_ref() };
    view.window()
        .and_then(|window| window.tabGroup())
        .is_some_and(|group| group.isTabBarVisible())
}
