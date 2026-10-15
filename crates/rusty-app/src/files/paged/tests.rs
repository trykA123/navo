use std::fs;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_ui::window_view::{Kinds, PagedKind, Shape, Sniff};

use super::{head, paged};

fn file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> Location {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

fn opens_as(location: &Location) -> Shape {
    let head = head(location).unwrap();
    let sniff = Sniff {
        head: &head.bytes,
        name: "",
        len: head.len,
    };
    Kinds::builtin().choose(&sniff)[0].shape(&sniff)
}

#[test]
fn small_text_opens_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let text = file(&dir, "notes.txt", b"hello\n");
    assert_eq!(opens_as(&text), Shape::Editor);
    assert!(paged(&text).unwrap().editable);
}

#[test]
fn a_nul_byte_or_utf16_opens_as_hex() {
    let dir = tempfile::tempdir().unwrap();
    let binary = file(&dir, "tool.exe", b"MZ\x90\x00\x03");
    let utf16 = file(&dir, "wide.txt", &[0xFF, 0xFE, b'a', 0]);
    assert_eq!(opens_as(&binary), Shape::Paged(PagedKind::Hex));
    assert_eq!(opens_as(&utf16), Shape::Paged(PagedKind::Hex));
    assert!(!paged(&binary).unwrap().editable);
}

#[test]
fn text_over_16_mb_opens_paged() {
    let dir = tempfile::tempdir().unwrap();
    let big = file(&dir, "big.log", &b"line\n".repeat(3_400_000));
    assert_eq!(opens_as(&big), Shape::Paged(PagedKind::Text));
    let sniffed = head(&big).unwrap();
    assert_eq!((sniffed.bytes.len(), sniffed.len), (8 * 1024, 17_000_000));
    let view = paged(&big).unwrap();
    assert!(!view.editable);
    assert_eq!(view.source.len(), 17_000_000);
}

#[test]
fn folders_and_missing_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let folder = Location::new(dir.path().to_string_lossy().into_owned()).unwrap();
    let missing = Location::new(dir.path().join("gone").to_string_lossy().into_owned()).unwrap();
    assert!(head(&folder).is_err());
    assert!(head(&missing).is_err());
}

#[test]
fn reopen_and_index_read_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let text = file(&dir, "a.txt", b"one\ntwo\nthree");
    let view = paged(&text).unwrap();
    assert_eq!((view.reopen)().unwrap().len(), 13);
    let cancel = AtomicBool::new(false);
    let index = (view.index)(&cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!(index.newlines(), 2);
}

#[test]
fn an_elf_file_gets_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let mut header = vec![0u8; 64];
    header[..6].copy_from_slice(b"\x7fELF\x02\x01");
    header[16] = 2;
    header[18] = 62;
    let elf = file(&dir, "tool", &header);
    let summary = paged(&elf).unwrap().summary;
    assert_eq!(summary.as_deref(), Some("ELF 64-bit · x86-64 · program"));
}
