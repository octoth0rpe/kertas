use std::path::PathBuf;

use kami::buffer::{Buffer, LoadError};

fn tmp_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[test]
fn load_round_trips_file_bytes() {
    let path = tmp_path("round_trip.md");
    let original = "\u{FEFF}# Heading\r\n\r\n- item\n- 項目\r\nno trailing newline".as_bytes();
    std::fs::write(&path, original).unwrap();

    let buffer = Buffer::load(&path).unwrap();
    let mut written = Vec::new();
    buffer.write_to(&mut written).unwrap();

    assert_eq!(written, original);
}

#[test]
fn load_missing_file_is_io_error() {
    let err = Buffer::load(tmp_path("does_not_exist.md")).unwrap_err();
    assert!(matches!(err, LoadError::Io(_)));
    assert!(err.to_string().starts_with("couldn't read file:"));
}
