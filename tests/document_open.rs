use std::path::PathBuf;

use kami::buffer::LoadError;
use kami::document::Document;

fn tmp_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[test]
fn opens_md_and_markdown_files() {
    for name in ["open.md", "open.markdown"] {
        let path = tmp_path(name);
        std::fs::write(&path, "# Hello\n").unwrap();
        let document = Document::open(&path).unwrap();
        assert_eq!(document.path(), Some(path.as_path()));
        assert_eq!(document.display_name(), name);
        assert_eq!(document.buffer().len_bytes(), 8);
    }
}

#[test]
fn invalid_utf8_names_the_file_and_problem() {
    let path = tmp_path("latin1.md");
    std::fs::write(&path, b"caf\xE9\n").unwrap();
    let err = Document::open(&path).unwrap_err();
    assert_eq!(err.display_name(), "latin1.md");
    assert!(matches!(err.source, LoadError::InvalidUtf8 { line: 1, .. }));
}

#[test]
fn missing_file_is_an_error() {
    let err = Document::open(tmp_path("missing.md")).unwrap_err();
    assert!(matches!(err.source, LoadError::Io(_)));
}

#[test]
fn directory_is_an_error() {
    let err = Document::open(PathBuf::from(env!("CARGO_TARGET_TMPDIR"))).unwrap_err();
    assert!(matches!(err.source, LoadError::Io(_)));
}
