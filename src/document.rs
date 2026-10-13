//! An open document: a file's path and its text.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, LoadError};

/// A Markdown document, backed by a file once it has been saved.
#[derive(Debug, Clone)]
pub struct Document {
    /// `None` until an untitled document is first saved.
    path: Option<PathBuf>,
    buffer: Buffer,
    /// The text as last loaded or saved, to tell whether there are unsaved
    /// changes. Comparing text (rather than counting edits) means undoing
    /// back to the saved text counts as unchanged, as on macOS.
    saved: String,
}

impl Document {
    /// A new, empty document that hasn't been saved.
    pub fn untitled() -> Self {
        Self {
            path: None,
            buffer: Buffer::default(),
            saved: String::new(),
        }
    }

    /// Opens the file at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, OpenError> {
        let path = path.into();
        match Buffer::load(&path) {
            Ok(buffer) => {
                let saved = buffer.slice(0..buffer.len_bytes());
                Ok(Self {
                    path: Some(path),
                    buffer,
                    saved,
                })
            }
            Err(source) => Err(OpenError { path, source }),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    /// The name to show for this document, e.g. in the window title.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .map_or_else(|| "Untitled".to_owned(), display_name)
    }

    /// Whether the text differs from what was last loaded or saved.
    pub fn is_dirty(&self) -> bool {
        self.buffer.rope() != self.saved.as_str()
    }

    /// Saves to the document's path. Fails for an untitled document; use
    /// [`Self::save_as`].
    pub fn save(&mut self) -> Result<(), SaveError> {
        let Some(path) = self.path.clone() else {
            return Err(SaveError {
                path: PathBuf::new(),
                source: io::Error::new(io::ErrorKind::NotFound, "the document has no file yet"),
            });
        };
        self.save_as(path)
    }

    /// Saves to `path`, which becomes the document's path.
    ///
    /// The buffer's bytes are written exactly as they are (F2). The file is
    /// replaced atomically: the text is written to a temporary file next to
    /// it, which is then renamed over it, so a failed save never leaves a
    /// half-written file. An existing file's permissions are kept, and a
    /// symlink is written through rather than replaced.
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), SaveError> {
        let path = path.into();
        write_atomically(&path, &self.buffer).map_err(|source| SaveError {
            path: path.clone(),
            source,
        })?;
        self.saved = self.buffer.slice(0..self.buffer.len_bytes());
        self.path = Some(path);
        Ok(())
    }
}

fn write_atomically(path: &Path, buffer: &Buffer) -> io::Result<()> {
    // Write through symlinks to the file they point at.
    let target = match fs::canonicalize(path) {
        Ok(target) => target,
        Err(err) if err.kind() == io::ErrorKind::NotFound => path.to_owned(),
        Err(err) => return Err(err),
    };
    let existing_permissions = fs::metadata(&target).ok().map(|meta| meta.permissions());

    let directory = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let temp = directory.join(format!(
        ".{}.kami-save-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        buffer.write_to(&mut file)?;
        file.flush()?;
        file.sync_all()?;
        if let Some(permissions) = existing_permissions {
            fs::set_permissions(&temp, permissions)?;
        }
        fs::rename(&temp, &target)
    })();
    if result.is_err() {
        fs::remove_file(&temp).ok();
    }
    result
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

/// A document couldn't be saved.
#[derive(Debug)]
pub struct SaveError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl SaveError {
    /// The name of the file that couldn't be saved.
    pub fn display_name(&self) -> String {
        display_name(&self.path)
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't save {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for SaveError {
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

    #[test]
    fn untitled_document() {
        let mut document = Document::untitled();
        assert_eq!(document.display_name(), "Untitled");
        assert!(document.path().is_none());
        assert!(!document.is_dirty());
        document.buffer_mut().replace(0..0, "text");
        assert!(document.is_dirty());
        assert!(document.save().is_err(), "untitled needs save_as");
    }
}
