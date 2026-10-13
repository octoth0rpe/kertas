//! Where a link in a document leads, so clicking it can follow it.

use std::path::{Path, PathBuf};

/// What following a link does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A Markdown file, opened in Kertas.
    Markdown(PathBuf),
    /// Another file or a folder, opened in its default app.
    File(PathBuf),
    /// A web link, `mailto:` or another URL, opened by the system.
    Url(String),
    /// A heading in the same document (`#setup`), or an empty link.
    /// Jumping to headings isn't supported yet, so nothing happens.
    SameDocument,
    /// A file that doesn't exist.
    Missing(PathBuf),
    /// A relative path in a document that hasn't been saved, so there's
    /// no folder to find it in.
    NoFolder(String),
}

/// Resolves a link's `destination` for the document saved at `document`
/// (`None` if it's untitled). Relative paths are relative to the document's
/// folder. A heading or query after a path (`other.md#setup`) is ignored.
pub fn resolve(destination: &str, document: Option<&Path>) -> LinkTarget {
    let destination = destination.trim();
    let path = match scheme(destination) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => {
            let rest = &destination[scheme.len() + 1..];
            let rest = rest
                .strip_prefix("//localhost")
                .or_else(|| rest.strip_prefix("//"))
                .unwrap_or(rest);
            percent_decode(strip_fragment(rest))
        }
        Some(_) => return LinkTarget::Url(destination.to_owned()),
        None => {
            let relative = strip_fragment(destination);
            if relative.is_empty() {
                return LinkTarget::SameDocument;
            }
            let path = percent_decode(relative);
            if path.is_absolute() {
                path
            } else {
                match document.and_then(Path::parent) {
                    Some(folder) => folder.join(path),
                    None => return LinkTarget::NoFolder(relative.to_owned()),
                }
            }
        }
    };
    if !path.exists() {
        LinkTarget::Missing(path)
    } else if is_markdown(&path) && path.is_file() {
        LinkTarget::Markdown(path)
    } else {
        LinkTarget::File(path)
    }
}

/// A URL's scheme, like `https` in `https://…`: letters, digits, `+`, `-`
/// and `.`, starting with a letter, followed by a colon.
fn scheme(destination: &str) -> Option<&str> {
    let (scheme, _) = destination.split_once(':')?;
    let mut chars = scheme.chars();
    let valid = chars.next()?.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// A path without the heading (`#…`) or query (`?…`) after it.
fn strip_fragment(path: &str) -> &str {
    path.split(['#', '?']).next().unwrap_or_default()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// Percent-decodes a URL path to bytes, since paths aren't necessarily
/// UTF-8. A `%` that isn't followed by two hex digits is kept.
pub fn percent_decode(path: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;

    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| path.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    PathBuf::from(std::ffi::OsString::from_vec(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with `notes.md`, `My Notes.markdown`, `paper.pdf` and a
    /// `sub` folder holding `deeper.md`, removed when dropped.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("kertas-links-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("sub")).unwrap();
            for file in [
                "notes.md",
                "My Notes.markdown",
                "paper.pdf",
                "sub/deeper.md",
            ] {
                std::fs::write(path.join(file), "").unwrap();
            }
            Self(path)
        }

        fn document(&self) -> PathBuf {
            self.0.join("sub/deeper.md")
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn urls_open_in_the_system() {
        for url in [
            "https://example.com/a.md",
            "http://example.com",
            "mailto:me@example.com",
            "x-apple.systempreferences:com.apple.preference",
        ] {
            assert_eq!(resolve(url, None), LinkTarget::Url(url.to_owned()));
        }
    }

    #[test]
    fn relative_paths_resolve_against_the_document() {
        let folder = Folder::new("relative");
        let document = folder.document();
        let document = Some(document.as_path());
        assert_eq!(
            resolve("../notes.md", document),
            LinkTarget::Markdown(folder.0.join("sub/../notes.md"))
        );
        assert_eq!(
            resolve("deeper.md#setup", document),
            LinkTarget::Markdown(folder.0.join("sub/deeper.md"))
        );
        assert_eq!(
            resolve("../My%20Notes.markdown", document),
            LinkTarget::Markdown(folder.0.join("sub/../My Notes.markdown"))
        );
        assert_eq!(
            resolve("../paper.pdf", document),
            LinkTarget::File(folder.0.join("sub/../paper.pdf"))
        );
        assert_eq!(
            resolve("..", document),
            LinkTarget::File(folder.0.join("sub/.."))
        );
        assert_eq!(
            resolve("missing.md", document),
            LinkTarget::Missing(folder.0.join("sub/missing.md"))
        );
    }

    #[test]
    fn absolute_paths_and_file_urls() {
        let folder = Folder::new("absolute");
        let notes = folder.0.join("notes.md");
        let path = notes.to_str().unwrap();
        assert_eq!(resolve(path, None), LinkTarget::Markdown(notes.clone()));
        assert_eq!(
            resolve(&format!("file://{path}#top"), None),
            LinkTarget::Markdown(notes.clone())
        );
        assert_eq!(
            resolve(&format!("file://localhost{path}"), None),
            LinkTarget::Markdown(notes)
        );
    }

    #[test]
    fn untitled_documents_have_no_folder() {
        assert_eq!(
            resolve("notes.md", None),
            LinkTarget::NoFolder("notes.md".to_owned())
        );
    }

    #[test]
    fn headings_in_the_same_document() {
        assert_eq!(resolve("#setup", None), LinkTarget::SameDocument);
        assert_eq!(resolve("", None), LinkTarget::SameDocument);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(
            percent_decode("/My%20Notes/caf%C3%A9.md"),
            PathBuf::from("/My Notes/café.md")
        );
        assert_eq!(percent_decode("/tmp/100%"), PathBuf::from("/tmp/100%"));
        assert_eq!(percent_decode("/tmp/%zz"), PathBuf::from("/tmp/%zz"));
    }
}
