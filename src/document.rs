//! An open document: a file's path and its text.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, LoadError};

/// A Markdown document, usually backed by a file.
#[derive(Debug, Clone)]
pub struct Document {
    path: PathBuf,
    buffer: Buffer,
}

impl Document {
    /// Opens the file at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, OpenError> {
        let path = path.into();
        match Buffer::load(&path) {
            Ok(buffer) => Ok(Self { path, buffer }),
            Err(source) => Err(OpenError { path, source }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// The name to show for this document, e.g. in the window title.
    pub fn display_name(&self) -> String {
        display_name(&self.path)
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// A document couldn't be opened.
#[derive(Debug)]
pub struct OpenError {
    pub path: PathBuf,
    pub source: LoadError,
}

impl OpenError {
    /// The name of the file that couldn't be opened.
    pub fn display_name(&self) -> String {
        display_name(&self.path)
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't open {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_is_file_name() {
        assert_eq!(display_name(Path::new("/a/b/notes.md")), "notes.md");
        assert_eq!(
            display_name(Path::new("relative.markdown")),
            "relative.markdown"
        );
    }
}
