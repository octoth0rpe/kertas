use std::fs;
use std::path::PathBuf;

use kami::document::Document;

/// A fresh, empty directory for one test.
fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("save-{name}"));
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn save_writes_bytes_exactly() {
    let path = test_dir("exact").join("doc.md");
    let original = "\u{FEFF}# Title\r\n\r\nno trailing newline".as_bytes();
    fs::write(&path, original).unwrap();

    let mut document = Document::open(&path).unwrap();
    document.save().unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        original,
        "unedited save is identical"
    );

    let len = document.buffer().len_bytes();
    document.buffer_mut().replace(len..len, "!");
    document.save().unwrap();
    let mut expected = original.to_vec();
    expected.push(b'!');
    assert_eq!(fs::read(&path).unwrap(), expected, "only the edit changes");
}

#[test]
fn dirty_tracks_unsaved_changes() {
    let path = test_dir("dirty").join("doc.md");
    fs::write(&path, "text").unwrap();
    let mut document = Document::open(&path).unwrap();
    assert!(!document.is_dirty());

    document.buffer_mut().replace(0..0, "more ");
    assert!(document.is_dirty());
    document.buffer_mut().replace(0..5, "");
    assert!(!document.is_dirty(), "back to the saved text");

    document.buffer_mut().replace(0..0, "x");
    document.save().unwrap();
    assert!(!document.is_dirty());
}

#[test]
fn save_as_sets_path_and_creates_file() {
    let dir = test_dir("save-as");
    let mut document = Document::untitled();
    document.buffer_mut().replace(0..0, "# New\n");
    let path = dir.join("new.md");
    document.save_as(&path).unwrap();
    assert_eq!(document.path(), Some(path.as_path()));
    assert_eq!(document.display_name(), "new.md");
    assert_eq!(fs::read_to_string(&path).unwrap(), "# New\n");
    assert!(!document.is_dirty());
}

#[test]
fn save_leaves_no_temporary_files() {
    let dir = test_dir("no-temp");
    let path = dir.join("doc.md");
    fs::write(&path, "a").unwrap();
    let mut document = Document::open(&path).unwrap();
    document.buffer_mut().replace(0..0, "b");
    document.save().unwrap();
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["doc.md"]);
}

#[cfg(unix)]
#[test]
fn save_keeps_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let path = test_dir("permissions").join("doc.md");
    fs::write(&path, "a").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let mut document = Document::open(&path).unwrap();
    document.buffer_mut().replace(0..0, "b");
    document.save().unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
}

#[cfg(unix)]
#[test]
fn save_writes_through_symlinks() {
    let dir = test_dir("symlink");
    let target = dir.join("real.md");
    let link = dir.join("link.md");
    fs::write(&target, "a").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let mut document = Document::open(&link).unwrap();
    document.buffer_mut().replace(0..0, "b");
    document.save().unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "ba");
}

#[test]
fn failed_save_reports_the_file() {
    let dir = test_dir("missing-dir");
    let mut document = Document::untitled();
    let path = dir.join("no-such-dir").join("doc.md");
    let err = document.save_as(&path).unwrap_err();
    assert_eq!(err.display_name(), "doc.md");
    assert!(
        document.path().is_none(),
        "path unchanged after a failed save"
    );
}

#[test]
fn new_document_at_a_path_creates_it_on_save() {
    let path = test_dir("new-at").join("new idea.md");
    let mut document = Document::new_at(&path);
    assert_eq!(document.display_name(), "new idea.md");
    assert!(!document.is_dirty());
    assert!(!path.exists(), "not created until saved");
    document.buffer_mut().replace(0..0, "# Idea\n");
    document.save().unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "# Idea\n");
}
