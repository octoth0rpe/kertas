//! Grouping document windows into native macOS tabs.
//!
//! GPUI only tabs new windows when the macOS "Prefer tabs when opening
//! documents" preference allows it (by default, only in full screen). Kami
//! has its own tabs/windows setting (REQUIREMENTS §8.2), so it adds the new
//! window as a tab itself, and turns automatic tabbing off. Otherwise macOS
//! adds "Show Tab Bar" and "Show All Tabs" to the View menu, which do
//! nothing useful with Kami's own tab strip.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow, NSWindowOrderingMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The `NSWindow` behind a GPUI window. `None` off the main thread, where
/// AppKit can't be used.
pub fn ns_window(window: &impl HasWindowHandle) -> Option<Retained<NSWindow>> {
    MainThreadMarker::new()?;
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: GPUI's AppKit handle points at the window's content view,
    // which lives as long as the GPUI window, borrowed here; and we're on
    // the main thread (checked above). `window()` returns a retained
    // NSWindow, so the result stays valid after the borrow ends.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
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

/// How many times [`add_as_tab`] was asked to group two windows, for the
/// smoke test.
#[cfg(debug_assertions)]
pub static TABBING_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Adds `tab` as a tab of `host`'s tab group, if it isn't in it already.
pub fn add_as_tab(host: &NSWindow, tab: &NSWindow) {
    if host == tab {
        return;
    }
    #[cfg(debug_assertions)]
    TABBING_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(group) = host.tabbedWindows()
        && group.iter().any(|window| &*window == tab)
    {
        return;
    }
    host.addTabbedWindow_ordered(tab, NSWindowOrderingMode::Above);
    tab.makeKeyAndOrderFront(None);
}

/// How many windows are in `window`'s tab group (1 if it isn't tabbed).
#[cfg(debug_assertions)]
pub fn tab_group_size(window: &NSWindow) -> usize {
    window.tabbedWindows().map_or(1, |group| group.count())
}

/// The position of the selected tab in `window`'s tab group.
#[cfg(debug_assertions)]
pub fn selected_tab(window: &NSWindow) -> Option<usize> {
    let group = window.tabGroup()?;
    let selected = group.selectedWindow()?;
    group.windows().iter().position(|window| window == selected)
}
