use std::sync::atomic::AtomicBool;

use crate::find::{Compiled, FileHits, FindQuery, Skip, scan};

fn search(bytes: &[u8], text: &str) -> FileHits {
    let compiled = Compiled::new(&FindQuery::parse(text)).unwrap();
    scan(bytes, &compiled, &AtomicBool::new(false)).unwrap()
}

#[test]
fn cp437_groesse_is_found_only_with_enc_dos() {
    let bytes = b"Gr\x94\xe1e";
    let dos = search(bytes, "Größe enc:dos");
    assert_eq!(dos.total, 1);
    assert_eq!(dos.hits[0].text, "Größe");
    assert_eq!(dos.hits[0].column, 1);
    assert_eq!(search(bytes, "Größe enc:utf8").total, 0);
    assert_eq!(search(bytes, "Größe enc:ansi").total, 0);
    assert_eq!(search(bytes, "Größe").total, 0);
}

#[test]
fn utf16le_without_bom_is_found_with_enc_utf16() {
    let mut bytes = Vec::new();
    for unit in "Größe hello".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let found = search(&bytes, "hello enc:utf16");
    assert_eq!(found.total, 1);
    assert_eq!(found.skipped, None);
    assert_eq!(found.hits[0].text, "Größe hello");
    assert_eq!(search(&bytes, "hello").skipped, Some(Skip::Binary));
}

#[test]
fn enc_ansi_finds_windows_1252() {
    let found = search(b"Gr\xf6\xdfe", "Größe enc:ansi");
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].text, "Größe");
}

#[test]
fn multiple_narrow_encodings_deduplicate_ascii_hits() {
    let found = search(b"hello hello\nhello", "hello enc:utf8,dos");
    assert_eq!(found.total, 3);
    assert_eq!(found.hits.len(), 3);
    assert_eq!(found.hits[0].line, 1);
    assert_eq!(found.hits[0].column, 1);
    assert_eq!(found.hits[1].column, 7);
    assert_eq!(found.hits[2].line, 2);
    assert_eq!(found.hits[2].column, 1);
}
