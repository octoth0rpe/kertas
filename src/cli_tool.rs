//! Installing the `kami` command line tool (#27): a link at
//! `/usr/local/bin/kami` to the script inside Kami.app
//! (`Contents/Resources/kami`, from `macos/kami`).
//!
//! `/usr/local/bin` usually belongs to the system, so installing and
//! uninstalling fall back to asking for an administrator password, with the
//! standard macOS prompt.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LINK: &str = "/usr/local/bin/kami";

/// Whether the command is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The link points at this Kami.app's script.
    Installed,
    /// The link points at another copy of Kami.app.
    OtherKami,
    /// Nothing is at the link's path.
    NotInstalled,
    /// Something that isn't Kami's is at the link's path.
    Taken,
    /// Kami isn't running as Kami.app (e.g. from `cargo run`), so there's no
    /// script to link to.
    NotInstalledApp,
}

pub fn status() -> Status {
    let Some(script) = script() else {
        return Status::NotInstalledApp;
    };
    let link = Path::new(LINK);
    match fs::read_link(link) {
        Ok(target) if target == script => Status::Installed,
        Ok(target) if target.ends_with("Kami.app/Contents/Resources/kami") => Status::OtherKami,
        Ok(_) => Status::Taken,
        Err(_) if link.symlink_metadata().is_ok() => Status::Taken,
        Err(_) => Status::NotInstalled,
    }
}

/// Links the command to this Kami.app, replacing a link to another copy.
/// Blocks while macOS asks for a password, so call it off the main thread.
pub fn install() -> Result<(), String> {
    let script = script().ok_or("Kami isn't running as Kami.app")?;
    if !matches!(status(), Status::NotInstalled | Status::OtherKami) {
        return Ok(());
    }
    let replace = || -> io::Result<()> {
        fs::remove_file(LINK).or_else(|err| match err.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(err),
        })?;
        std::os::unix::fs::symlink(&script, LINK)
    };
    match replace() {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => as_administrator(
            &format!(
                "mkdir -p /usr/local/bin && ln -sfn {} {LINK}",
                shell_quote(&script.to_string_lossy())
            ),
            "Kami wants to install the kami command in /usr/local/bin.",
        ),
        Err(err) => Err(err.to_string()),
    }
}

/// Removes the command, if it's Kami's.
pub fn uninstall() -> Result<(), String> {
    if !matches!(status(), Status::Installed | Status::OtherKami) {
        return Ok(());
    }
    match fs::remove_file(LINK) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => as_administrator(
            &format!("rm -f {LINK}"),
            "Kami wants to remove the kami command from /usr/local/bin.",
        ),
        Err(err) => Err(err.to_string()),
    }
}

/// This Kami.app's script, if Kami is running as an app bundle.
fn script() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    // Kami.app/Contents/MacOS/Kami → Kami.app/Contents/Resources/kami
    let contents = executable.parent()?.parent()?;
    let script = contents.join("Resources/kami");
    (contents.parent()?.extension()? == "app" && script.is_file()).then_some(script)
}

/// Runs `command` as root, after macOS's administrator password prompt.
fn as_administrator(command: &str, prompt: &str) -> Result<(), String> {
    let script = format!(
        "do shell script {} with prompt {} with administrator privileges",
        applescript_quote(command),
        applescript_quote(prompt)
    );
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn applescript_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', r"\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("/A/Kami's.app"), r"'/A/Kami'\''s.app'");
        assert_eq!(applescript_quote(r#"say "hi" \ "#), r#""say \"hi\" \\ ""#);
    }
}
