//! Whether Kertas is the default app for Markdown files, and making it so
//! (#23). macOS treats `.md` and `.markdown` as one type.

use std::sync::Mutex;

use block2::RcBlock;
use futures::channel::oneshot;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_uniform_type_identifiers::UTType;

const MARKDOWN: &str = "net.daringfireball.markdown";

/// Who opens Markdown files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Kertas does.
    Kertas,
    /// Another app does (its name, if there is one).
    Other(Option<String>),
    /// Kertas isn't running as an installed app (e.g. from `cargo run`), so it
    /// can't be the default.
    NotInstalled,
}

pub fn status() -> Status {
    let bundle = NSBundle::mainBundle();
    let Some(kertas) = bundle.bundleIdentifier() else {
        return Status::NotInstalled;
    };
    let Some(markdown) = markdown_type() else {
        return Status::Other(None);
    };
    let Some(app) = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(&markdown)
    else {
        return Status::Other(None);
    };
    let identifier = NSBundle::bundleWithURL(&app).and_then(|app| app.bundleIdentifier());
    if identifier.is_some_and(|identifier| identifier == kertas) {
        Status::Kertas
    } else {
        let name = app
            .lastPathComponent()
            .map(|name| name.to_string().trim_end_matches(".app").to_owned());
        Status::Other(name)
    }
}

/// Asks macOS to make Kertas the default app for Markdown files. macOS asks
/// the user to confirm. Resolves to an error message if it didn't happen.
pub fn make_default() -> oneshot::Receiver<Result<(), String>> {
    let (done, result) = oneshot::channel();
    let Some(markdown) = markdown_type() else {
        done.send(Err("macOS doesn't know the Markdown file type".into()))
            .ok();
        return result;
    };
    let kertas = NSBundle::mainBundle().bundleURL();
    // The completion handler may be called on any thread, once.
    let done = Mutex::new(Some(done));
    let handler = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: macOS passes a valid error or null.
        let error = unsafe { error.as_ref() };
        let result = match error {
            None => Ok(()),
            Some(error) => Err(error.localizedDescription().to_string()),
        };
        if let Some(done) = done.lock().ok().and_then(|mut done| done.take()) {
            done.send(result).ok();
        }
    });
    // AppKit copies the block, so dropping `handler` when this returns is
    // fine; the copy keeps `done` alive until the handler has run.
    NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenContentType_completionHandler(
        &kertas,
        &markdown,
        Some(&handler),
    );
    result
}

fn markdown_type() -> Option<objc2::rc::Retained<UTType>> {
    UTType::typeWithIdentifier(&NSString::from_str(MARKDOWN))
}
