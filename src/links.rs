//! Where a link in a document leads, so clicking it can follow it.

use std::path::{Path, PathBuf};

/// What following a link does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A Markdown file, opened in Kertas, and the heading or anchor to
    /// jump to, if there is one (`other.md#setup`).
    Markdown(PathBuf, Option<String>),
    /// Another file or a folder, opened in its default app.
    File(PathBuf),
    /// A web link, `mailto:` or another URL, opened by the system.
    Url(String),
    /// A heading or anchor in the same document (`#setup`), or `None` for
    /// an empty link, which does nothing.
    SameDocument(Option<String>),
    /// A file that doesn't exist.
    Missing(PathBuf),
    /// A relative path in a document that hasn't been saved, so there's
    /// no folder to find it in.
    NoFolder(String),
}

/// Resolves a link's `destination` for the document saved at `document`
/// (`None` if it's untitled). Relative paths are relative to the document's
/// folder. A query after a path (`other.md?x`) is ignored.
pub fn resolve(destination: &str, document: Option<&Path>) -> LinkTarget {
    let destination = destination.trim();
    let anchor = anchor(destination);
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
                return LinkTarget::SameDocument(anchor);
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
        LinkTarget::Markdown(path, anchor)
    } else {
        LinkTarget::File(path)
    }
}

/// The local file an image's `destination` points to, resolved as
/// [`resolve`] resolves links. `None` for web images (#92 shows local
/// images only) and other URLs, and for relative paths in an untitled
/// document. The file may not exist.
pub fn image_path(destination: &str, document: Option<&Path>) -> Option<PathBuf> {
    match resolve(destination, document) {
        LinkTarget::Markdown(path, _) | LinkTarget::File(path) | LinkTarget::Missing(path) => {
            Some(path)
        }
        LinkTarget::Url(_) | LinkTarget::SameDocument(_) | LinkTarget::NoFolder(_) => None,
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

/// The heading or anchor a link points to: what follows its `#`,
/// percent-decoded, or `None` if there's nothing there.
fn anchor(destination: &str) -> Option<String> {
    let (_, fragment) = destination.split_once('#')?;
    let decoded = percent_decode(fragment);
    let anchor = decoded.to_string_lossy();
    (!anchor.is_empty()).then(|| anchor.into_owned())
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
            LinkTarget::Markdown(folder.0.join("sub/../notes.md"), None)
        );
        assert_eq!(
            resolve("deeper.md#setup", document),
            LinkTarget::Markdown(folder.0.join("sub/deeper.md"), Some("setup".to_owned()))
        );
        assert_eq!(
            resolve("../My%20Notes.markdown", document),
            LinkTarget::Markdown(folder.0.join("sub/../My Notes.markdown"), None)
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
        assert_eq!(
            resolve(path, None),
            LinkTarget::Markdown(notes.clone(), None)
        );
        assert_eq!(
            resolve(&format!("file://{path}#top"), None),
            LinkTarget::Markdown(notes.clone(), Some("top".to_owned()))
        );
        assert_eq!(
            resolve(&format!("file://localhost{path}"), None),
            LinkTarget::Markdown(notes, None)
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
        assert_eq!(
            resolve("#setup", None),
            LinkTarget::SameDocument(Some("setup".to_owned()))
        );
        assert_eq!(
            resolve("#caf%C3%A9", None),
            LinkTarget::SameDocument(Some("café".to_owned()))
        );
        assert_eq!(resolve("", None), LinkTarget::SameDocument(None));
        assert_eq!(resolve("#", None), LinkTarget::SameDocument(None));
    }

    #[test]
    fn image_paths() {
        let folder = Folder::new("images");
        let document = folder.document();
        let document = Some(document.as_path());
        assert_eq!(
            image_path("../paper.pdf", document),
            Some(folder.0.join("sub/../paper.pdf"))
        );
        assert_eq!(
            image_path("missing.png", document),
            Some(folder.0.join("sub/missing.png"))
        );
        assert_eq!(image_path("https://example.com/a.png", document), None);
        assert_eq!(image_path("a.png", None), None);
        assert_eq!(
            image_path("/abs/a.png", None),
            Some(PathBuf::from("/abs/a.png"))
        );
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
