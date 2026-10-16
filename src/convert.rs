//! `kt --convert in.md out.pdf` (#94): saves a Markdown file as a PDF,
//! laid out as File > Print prints it, without starting the app.
//!
//! `kt` runs `Kertas --convert …` directly rather than through a running
//! Kertas, so it can wait for the result and report errors. Nothing here
//! touches GPUI: no window, no Dock icon, and no focus taken.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kertas::document::Document;
use kertas::model::Model;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSFontManager};

/// If the arguments ask for `--convert`, converts and returns the exit
/// code; otherwise `None`, and Kertas starts as usual.
pub fn run_if_requested() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != "--convert" {
        return None;
    }
    Some(match convert(args.collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("kt: {message}");
            ExitCode::FAILURE
        }
    })
}

fn convert(args: Vec<OsString>) -> Result<(), String> {
    let [input, output] = <[OsString; 2]>::try_from(args)
        .map_err(|_| "usage: kt --convert in.md out.pdf".to_owned())?;
    let (input, output) = (PathBuf::from(input), PathBuf::from(output));
    // The type comes from the extension, leaving room for others later.
    let is_pdf = output
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
    if !is_pdf {
        return Err(format!(
            "can't convert to {}: only .pdf is supported",
            output.display()
        ));
    }
    let document = Document::open(&input).map_err(|err| err.to_string())?;
    check_writable(&output)?;

    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    // Printing needs the shared application, but it must never show up in
    // the Dock or take focus.
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
    let families = NSFontManager::sharedFontManager(mtm).availableFontFamilies();
    let families: Vec<String> = families.iter().map(|family| family.to_string()).collect();
    let installed = |name: &str| {
        families
            .iter()
            .find(|family| family.eq_ignore_ascii_case(name))
            .cloned()
    };
    let settings = crate::app_settings::read_once(&installed);

    let model = Model::new(&document.buffer().rope().to_string());
    // Absolute paths, so images resolve and AppKit gets proper file URLs.
    let input = std::path::absolute(&input).unwrap_or(input);
    let output = std::path::absolute(&output).unwrap_or(output);
    if !crate::print::save_pdf(&model, Some(&input), &settings, &output, mtm) {
        return Err(format!("couldn't write {}", output.display()));
    }
    Ok(())
}

/// Checks that `output` can be written before doing the work, so a bad
/// path is reported clearly rather than as a failed print. Leaves an
/// existing file as it was.
fn check_writable(output: &Path) -> Result<(), String> {
    let folder = match output.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    };
    if !folder.is_dir() {
        return Err(format!(
            "can't write {}: {} isn't a folder",
            output.display(),
            folder.display()
        ));
    }
    if output.is_dir() {
        return Err(format!("can't write {}: it's a folder", output.display()));
    }
    let existed = output.exists();
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(output)
        .map_err(|err| format!("can't write {}: {err}", output.display()))?;
    if !existed {
        std::fs::remove_file(output).ok();
    }
    Ok(())
}
