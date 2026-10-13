//! The Dock icon, which follows the system's light or dark mode.
//!
//! An app bundle's `.icns` icon can't switch with the appearance (that
//! needs an Xcode asset catalog), so Kami sets its Dock icon itself while
//! it runs, updating it when macOS reports an appearance change. The icons
//! are built from `macos/icon-light.svg` and `macos/icon-dark.svg` by
//! `scripts/make-icon.sh`.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{
    NSData, NSDistributedNotificationCenter, NSNotification, NSOperationQueue, NSString,
    NSUserDefaults,
};

const LIGHT: &[u8] = include_bytes!("../macos/dock-light.png");
const DARK: &[u8] = include_bytes!("../macos/dock-dark.png");

/// Sets the Dock icon, and updates it whenever the system switches
/// between light and dark mode.
pub fn init() {
    set_icon(system_is_dark());
    let changed = RcBlock::new(|_: NonNull<NSNotification>| set_icon(system_is_dark()));
    let name = NSString::from_str("AppleInterfaceThemeChangedNotification");
    // SAFETY: the block captures nothing, and runs on the main queue,
    // where AppKit may be used.
    let observer = unsafe {
        NSDistributedNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&name),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &changed,
        )
    };
    // The notification center copies the block and keeps the observer
    // registered until it's removed. Kami observes for as long as it runs
    // and never removes it, so the token that would remove it is leaked
    // on purpose.
    std::mem::forget(observer);
}

/// Whether the system (not Kami's theme setting) is in dark mode. The Dock
/// follows the system, whatever Kami's own theme is.
fn system_is_dark() -> bool {
    let style = NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str("AppleInterfaceStyle"));
    style.is_some_and(|style| style.to_string() == "Dark")
}

fn set_icon(dark: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(if dark { DARK } else { LIGHT });
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    // SAFETY: called on the main thread with a valid image.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}
