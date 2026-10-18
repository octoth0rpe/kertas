//! The text buffer that holds a document's contents.
//!
//! The buffer stores the file's text exactly as loaded: line endings,
//! a byte-order mark and a missing trailing newline are all preserved, so
//! writing an unedited buffer reproduces the original bytes (F2).

use std::fmt;
use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;

use ropey::Rope;

/// A document's text.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    /// Creates a buffer from raw file bytes, which must be valid UTF-8.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(|err| {
            let offset = err.valid_up_to();
            LoadError::InvalidUtf8 {
                offset,
                line: line_number_at(&bytes[..offset]),
            }
        })?;
        Ok(Self {
            rope: Rope::from_str(text),
        })
    }

    /// Reads a file into a buffer.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let bytes = std::fs::read(path).map_err(LoadError::Io)?;
        Self::from_bytes(&bytes)
    }

    /// Writes the buffer's bytes, unchanged, to `writer`.
    pub fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        for chunk in self.rope.chunks() {
            writer.write_all(chunk.as_bytes())?;
        }
        Ok(())
    }

    /// The length of the text in bytes.
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Whether the buffer contains no text.
    pub fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    /// The underlying rope.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// The text in a byte range.
    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.byte_slice(range).to_string()
    }

    /// Replaces the text in a byte range. Both ends must be on character
    /// boundaries.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, text);
    }

    /// The UTF-16 offset of byte `offset`, which the platform's text input
    /// uses. An offset past the end maps to the end.
    pub fn byte_to_utf16(&self, offset: usize) -> usize {
        let offset = offset.min(self.rope.len_bytes());
        self.rope.char_to_utf16_cu(self.rope.byte_to_char(offset))
    }

    /// The byte offset of UTF-16 `offset`. An offset past the end maps to
    /// the end, and one inside a surrogate pair to the character's start.
    pub fn utf16_to_byte(&self, offset: usize) -> usize {
        let offset = offset.min(self.rope.len_utf16_cu());
        self.rope.char_to_byte(self.rope.utf16_cu_to_char(offset))
    }
}

/// 1-based line number of the position just after `prefix`.
fn line_number_at(prefix: &[u8]) -> usize {
    prefix.iter().filter(|&&b| b == b'\n').count() + 1
}

/// Why a file couldn't be loaded into a buffer.
#[derive(Debug)]
pub enum LoadError {
    /// The file couldn't be read.
    Io(io::Error),
    /// The file isn't valid UTF-8. `offset` is the byte offset of the first
    /// invalid byte, and `line` is the 1-based line it's on.
    InvalidUtf8 { offset: usize, line: usize },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(err) => write!(f, "couldn't read file: {err}"),
            LoadError::InvalidUtf8 { offset, line } => write!(
                f,
                "file is not valid UTF-8 (invalid byte on line {line}, byte offset {offset})"
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io(err) => Some(err),
            LoadError::InvalidUtf8 { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(input: &[u8]) -> Vec<u8> {
        let buffer = Buffer::from_bytes(input).expect("valid UTF-8");
        assert_eq!(buffer.len_bytes(), input.len());
        let mut out = Vec::new();
        buffer.write_to(&mut out).unwrap();
        out
    }

    #[track_caller]
    fn assert_round_trips(input: &[u8]) {
        assert_eq!(round_trip(input), input);
    }

    #[test]
    fn lf_line_endings() {
        assert_round_trips(b"# Title\n\nSome text.\n");
    }

    #[test]
    fn crlf_line_endings() {
        assert_round_trips(b"# Title\r\n\r\nSome text.\r\n");
    }

    #[test]
    fn mixed_line_endings() {
        assert_round_trips(b"one\r\ntwo\nthree\rfour\r\n\nfive");
    }

    #[test]
    fn no_trailing_newline() {
        assert_round_trips(b"no newline at end");
    }

    #[test]
    fn multiple_trailing_newlines() {
        assert_round_trips(b"text\n\n\n");
    }

    #[test]
    fn empty() {
        let buffer = Buffer::from_bytes(b"").unwrap();
        assert!(buffer.is_empty());
        assert_round_trips(b"");
    }

    #[test]
    fn non_ascii() {
        assert_round_trips("紙 — café, naïve, 🦀 emoji\n> 引用\n".as_bytes());
    }

    #[test]
    fn byte_order_mark() {
        assert_round_trips("\u{FEFF}# Starts with a BOM\n".as_bytes());
    }

    #[test]
    fn trailing_whitespace_and_tabs() {
        assert_round_trips(b"line with spaces   \n\tindented\n  \n");
    }

    #[test]
    fn large_document() {
        // Larger than a single rope chunk, so writing spans many chunks.
        let input: String = (0..10_000)
            .map(|i| format!("Line {i} with some text, ünïcödé\r\n"))
            .collect();
        assert_round_trips(input.as_bytes());
    }

    #[test]
    fn replace_edits_by_byte_range() {
        let mut buffer = Buffer::from_bytes("café 🦀 text".as_bytes()).unwrap();
        buffer.replace(6..10, "crab");
        assert_eq!(buffer.slice(0..buffer.len_bytes()), "café crab text");
        buffer.replace(0..0, "> ");
        buffer.replace(buffer.len_bytes()..buffer.len_bytes(), "\n");
        assert_eq!(buffer.slice(0..buffer.len_bytes()), "> café crab text\n");
    }

    #[test]
    fn invalid_utf8_reports_line_and_offset() {
        let err = Buffer::from_bytes(b"ok\nalso ok\nbad \xFF here\n").unwrap_err();
        match err {
            LoadError::InvalidUtf8 { offset, line } => {
                assert_eq!(offset, 15);
                assert_eq!(line, 3);
            }
            other => panic!("expected InvalidUtf8, got {other:?}"),
        }
        assert_eq!(
            err.to_string(),
            "file is not valid UTF-8 (invalid byte on line 3, byte offset 15)"
        );
    }

    #[test]
    fn truncated_multibyte_sequence_is_invalid() {
        // First two bytes of a three-byte character.
        let err = Buffer::from_bytes(b"abc\xE7\xB4").unwrap_err();
        assert!(matches!(err, LoadError::InvalidUtf8 { offset: 3, line: 1 }));
    }

    #[test]
    fn maps_offsets_to_and_from_utf16() {
        // "é" is two bytes and one UTF-16 unit; "😀" is four bytes and a
        // surrogate pair.
        let buffer = Buffer::from_bytes("aé😀\r\nb".as_bytes()).unwrap();
        let pairs = [(0, 0), (1, 1), (3, 2), (7, 4), (8, 5), (9, 6), (10, 7)];
        for (byte, utf16) in pairs {
            assert_eq!(buffer.byte_to_utf16(byte), utf16, "byte {byte}");
            assert_eq!(buffer.utf16_to_byte(utf16), byte, "UTF-16 {utf16}");
        }
        // Inside the surrogate pair: the emoji's start.
        assert_eq!(buffer.utf16_to_byte(3), 3);
        // Past the end: the end.
        assert_eq!(buffer.byte_to_utf16(99), 7);
        assert_eq!(buffer.utf16_to_byte(99), 10);
    }

    #[test]
    fn empty_buffer_maps_to_zero() {
        let buffer = Buffer::default();
        assert_eq!(buffer.byte_to_utf16(0), 0);
        assert_eq!(buffer.utf16_to_byte(5), 0);
    }
}
