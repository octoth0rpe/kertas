//! The Dock icon, which follows the system's light or dark mode.
//!
//! An app bundle's `.icns` icon can't switch with the appearance (that
//! needs an Xcode asset catalog), so Kami sets its Dock icon itself while
//! it runs. The icons are built from `macos/icon-light.svg` and
//! `macos/icon-dark.svg` by `scripts/make-icon.sh`.

use std::time::Duration;

use gpui::App;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{NSData, NSString, NSUserDefaults};

const LIGHT: &[u8] = include_bytes!("../macos/dock-light.png");
const DARK: &[u8] = include_bytes!("../macos/dock-dark.png");

/// How often to check for a change of system appearance.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Sets the Dock icon and keeps it matching the system appearance.
pub fn init(cx: &mut App) {
    let mut dark = system_is_dark();
    set_icon(dark);
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let now = cx.update(|_| system_is_dark());
            if now != dark {
                dark = now;
                cx.update(|_| set_icon(dark));
            }
        }
    })
    .detach();
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
