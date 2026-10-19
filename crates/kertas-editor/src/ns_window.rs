//! The `NSWindow` behind a GPUI window, for AppKit calls GPUI doesn't
//! make, such as printing as a sheet.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
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
