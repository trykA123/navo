use std::path::PathBuf;

use super::{ClipboardError, Effect, decode_drop_files, encode_drop_files};
#[cfg(unix)]
use super::{FileClipboard, clear_if_same, read_files, write_files};

fn ansi_block(paths: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 20];
    bytes[..4].copy_from_slice(&20u32.to_le_bytes());
    bytes.extend_from_slice(paths);
    bytes
}

#[test]
fn ascii_paths_keep_their_exact_form() {
    let paths = [r"C:\work\..\notes.txt", r"\\server\share\file.txt"].map(PathBuf::from);
    assert_eq!(
        decode_drop_files(&encode_drop_files(&paths)),
        Ok(paths.to_vec())
    );
}

#[test]
fn non_bmp_unicode_round_trips() {
    let paths = [PathBuf::from("C:\\資料\\🦀🎵.txt")];
    assert_eq!(
        decode_drop_files(&encode_drop_files(&paths)),
        Ok(paths.to_vec())
    );
}

#[test]
fn spaces_round_trip() {
    let paths = [PathBuf::from(r"C:\Program Files\a file.txt")];
    assert_eq!(
        decode_drop_files(&encode_drop_files(&paths)),
        Ok(paths.to_vec())
    );
}

#[test]
fn paths_longer_than_max_path_round_trip() {
    let paths = [PathBuf::from(format!("C:\\{}", "a".repeat(300)))];
    assert_eq!(
        decode_drop_files(&encode_drop_files(&paths)),
        Ok(paths.to_vec())
    );
}

#[test]
fn encoded_header_and_terminators_match_dropfiles() {
    let bytes = encode_drop_files(&[PathBuf::from(r"C:\a")]);
    assert_eq!(&bytes[..4], &20u32.to_le_bytes());
    assert_eq!(&bytes[4..16], &[0; 12]);
    assert_eq!(&bytes[16..20], &1u32.to_le_bytes());
    let units: Vec<_> = "C:\\a\0\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(&bytes[20..], units);
}

#[test]
fn ansi_blocks_decode() {
    let bytes = ansi_block(b"C:\\one.txt\0C:\\two files.txt\0\0");
    let paths = [r"C:\one.txt", r"C:\two files.txt"].map(PathBuf::from);
    assert_eq!(decode_drop_files(&bytes), Ok(paths.to_vec()));
}

#[test]
fn empty_lists_have_two_terminators() {
    let bytes = encode_drop_files(&[]);
    assert_eq!(&bytes[20..], &[0; 4]);
    assert_eq!(decode_drop_files(&bytes), Ok(Vec::new()));
    assert_eq!(decode_drop_files(&ansi_block(b"\0\0")), Ok(Vec::new()));
}

#[test]
fn truncated_headers_and_invalid_offsets_are_malformed() {
    let bytes = encode_drop_files(&[]);
    for len in 0..20 {
        assert_eq!(
            decode_drop_files(&bytes[..len]),
            Err(ClipboardError::Malformed)
        );
    }
    for offset in [0u32, 19, 21, 24, u32::MAX] {
        let mut invalid = bytes.clone();
        invalid[..4].copy_from_slice(&offset.to_le_bytes());
        assert_eq!(decode_drop_files(&invalid), Err(ClipboardError::Malformed));
    }
}

#[test]
fn padding_after_the_list_is_ignored() {
    let paths = vec![PathBuf::from(r"C:\a")];
    let mut bytes = encode_drop_files(&paths);
    bytes.extend_from_slice(&[0xAB, 0xCD, 0xEF]);
    assert_eq!(decode_drop_files(&bytes), Ok(paths));
}

#[test]
fn truncated_or_unterminated_lists_are_malformed() {
    let bytes = encode_drop_files(&[PathBuf::from(r"C:\a")]);
    for removed in 1..=4 {
        assert_eq!(
            decode_drop_files(&bytes[..bytes.len() - removed]),
            Err(ClipboardError::Malformed)
        );
    }
    for paths in [b"C:\\a".as_slice(), b"C:\\a\0", b"\0", b"\0C:\\a\0\0"] {
        assert_eq!(
            decode_drop_files(&ansi_block(paths)),
            Err(ClipboardError::Malformed)
        );
    }
}

#[test]
fn drop_effect_bytes_follow_explorer() {
    assert_eq!(Effect::Copy.to_bytes(), [5, 0, 0, 0]);
    assert_eq!(Effect::Move.to_bytes(), [2, 0, 0, 0]);
    assert_eq!(Effect::from_bytes(Some(&[5, 0, 0, 0])), Effect::Copy);
    assert_eq!(Effect::from_bytes(Some(&[2, 0, 0, 0])), Effect::Move);
    assert_eq!(Effect::from_bytes(Some(&[7, 0, 0, 0])), Effect::Move);
    assert_eq!(Effect::from_bytes(Some(&[0, 0, 0, 0])), Effect::Copy);
    assert_eq!(Effect::from_bytes(Some(&[0, 0, 0, 2])), Effect::Copy);
    assert_eq!(Effect::from_bytes(None), Effect::Copy);
}

#[cfg(unix)]
#[test]
fn clipboard_operations_are_unsupported_on_unix() {
    let files = FileClipboard {
        paths: vec![PathBuf::from("/tmp/one.txt")],
        effect: Effect::Copy,
    };
    assert_eq!(write_files(&files), Err(ClipboardError::Unsupported));
    assert_eq!(read_files(), Err(ClipboardError::Unsupported));
    assert_eq!(clear_if_same(&files), Err(ClipboardError::Unsupported));
}

#[test]
fn busy_errors_are_readable() {
    assert_eq!(
        ClipboardError::Busy.to_string(),
        "The clipboard is in use by another program"
    );
}
