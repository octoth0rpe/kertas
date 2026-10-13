//! An open document: a file's path and its text.

use std::fmt;
use std::fs;
use std::hash::Hasher;
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
    /// The version of the file last loaded, saved or accepted, or `None`
    /// if there was no file then. Changes made by other apps show up as a
    /// different version (F5).
    disk_version: Option<DiskVersion>,
}

/// Identifies a file's contents, to notice when they change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskVersion(u64);

impl DiskVersion {
    fn of(bytes: &[u8]) -> Self {
        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(bytes);
        Self(hasher.finish())
    }
}

/// How a document's file differs from the version Kami last loaded, saved
/// or accepted: another app changed it (F5).
#[derive(Debug)]
pub struct DiskChange {
    pub kind: DiskChangeKind,
    /// The file's new version, or `None` if it's gone.
    version: Option<DiskVersion>,
}

#[derive(Debug)]
pub enum DiskChangeKind {
    /// The file has new text.
    Changed(String),
    /// The file is gone: deleted, or renamed or moved elsewhere.
    Missing,
    /// The file changed, but can't be read as text.
    Unreadable(LoadError),
}

impl Document {
    /// A new, empty document that hasn't been saved.
    pub fn untitled() -> Self {
        Self {
            path: None,
            buffer: Buffer::default(),
            saved: String::new(),
            disk_version: None,
        }
    }

    /// A new, empty document that saves to `path`, which doesn't exist yet.
    /// Saving creates it.
    pub fn new_at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::untitled()
        }
    }

    /// A document holding `text`, with no file and no unsaved changes.
    pub fn from_text(text: &str) -> Self {
        let mut buffer = Buffer::default();
        buffer.replace(0..0, text);
        Self {
            path: None,
            buffer,
            saved: text.to_owned(),
            disk_version: None,
        }
    }

    /// Opens the file at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, OpenError> {
        let path = path.into();
        let loaded = std::fs::read(&path)
            .map_err(LoadError::Io)
            .and_then(|bytes| Ok((Buffer::from_bytes(&bytes)?, DiskVersion::of(&bytes))));
        match loaded {
            Ok((buffer, version)) => {
                let saved = buffer.slice(0..buffer.len_bytes());
                Ok(Self {
                    path: Some(path),
                    buffer,
                    saved,
                    disk_version: Some(version),
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
        self.disk_version = Some(DiskVersion::of(self.saved.as_bytes()));
        self.path = Some(path);
        Ok(())
    }

    /// How the file now differs from the version last loaded, saved or
    /// accepted, or `None` if it doesn't, or the document has no file.
    /// Kami's own saves never count as changes.
    pub fn disk_change(&self) -> Option<DiskChange> {
        let path = self.path.as_deref()?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return self.disk_version.is_some().then_some(DiskChange {
                    kind: DiskChangeKind::Missing,
                    version: None,
                });
            }
            Err(err) => {
                return Some(DiskChange {
                    kind: DiskChangeKind::Unreadable(LoadError::Io(err)),
                    version: self.disk_version,
                });
            }
        };
        let version = Some(DiskVersion::of(&bytes));
        if version == self.disk_version {
            return None;
        }
        let kind = match String::from_utf8(bytes) {
            Ok(text) => DiskChangeKind::Changed(text),
            // Buffer says where the bad bytes are.
            Err(err) => match Buffer::from_bytes(err.as_bytes()) {
                Err(err) => DiskChangeKind::Unreadable(err),
                Ok(buffer) => DiskChangeKind::Changed(buffer.slice(0..buffer.len_bytes())),
            },
        };
        Some(DiskChange { kind, version })
    }

    /// Accepts `change` without reloading ("Keep Mine"): it's no longer
    /// reported, and the next save overwrites the file. The document's
    /// text now counts as unsaved if it differs from the file's.
    pub fn accept_disk_change(&mut self, change: &DiskChange) {
        if let DiskChangeKind::Changed(text) = &change.kind {
            self.saved.clone_from(text);
        }
        self.disk_version = change.version;
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
    fn document_from_text_is_unchanged() {
        let document = Document::from_text("# Help\n");
        assert_eq!(document.buffer().slice(0..7), "# Help\n");
        assert!(!document.is_dirty());
        assert!(document.path().is_none());
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
        assert!(document.disk_change().is_none(), "untitled has no file");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kami-document-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn notices_changes_by_other_apps_but_not_its_own_saves() {
        let dir = temp_dir("disk-change");
        let path = dir.join("notes.md");
        fs::write(&path, "one\n").unwrap();
        let mut document = Document::open(&path).unwrap();
        assert!(document.disk_change().is_none());

        document.buffer_mut().replace(0..0, "zero\n");
        document.save().unwrap();
        assert!(document.disk_change().is_none(), "Kami's own save");

        fs::write(&path, "two\n").unwrap();
        let change = document.disk_change().expect("changed by another app");
        assert!(matches!(&change.kind, DiskChangeKind::Changed(text) if text == "two\n"));
        assert!(!document.is_dirty());

        // Keep Mine: no longer reported, and the text now differs from the
        // file's.
        document.accept_disk_change(&change);
        assert!(document.disk_change().is_none());
        assert!(document.is_dirty());

        fs::write(&path, b"\xff").unwrap();
        let change = document.disk_change().expect("unreadable");
        assert!(matches!(change.kind, DiskChangeKind::Unreadable(_)));
        document.accept_disk_change(&change);
        assert!(document.disk_change().is_none());

        fs::remove_file(&path).unwrap();
        let change = document.disk_change().expect("deleted");
        assert!(matches!(change.kind, DiskChangeKind::Missing));
        document.accept_disk_change(&change);
        assert!(document.disk_change().is_none());

        // Saving recreates it.
        document.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "zero\none\n");
        assert!(document.disk_change().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_new_file_is_missing_until_created() {
        let dir = temp_dir("new-file");
        let path = dir.join("new.md");
        let document = Document::new_at(&path);
        assert!(document.disk_change().is_none(), "not created yet");
        fs::write(&path, "made elsewhere").unwrap();
        assert!(matches!(
            document.disk_change().map(|change| change.kind),
            Some(DiskChangeKind::Changed(_))
        ));
        fs::remove_dir_all(&dir).ok();
    }
}
