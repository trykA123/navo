//! Tests of the Git side in checkout form: converted sizes in the list, converted bytes on
//! `open`, no object ids, attributes that keep the stored form, and a cancelled first build.

use std::io::{Read, Write};
use std::ops::ControlFlow;

use sidewise_core::ProviderPath;
use sidewise_engine::model::{CancelToken, EntryKind, ReadHint, RelPath, SideSource};

use super::*;
use crate::fixture::Fixture;

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

/// A fixture with `a.txt` = `"one\ntwo\n"`, `autocrlf = true` in its config and `attributes`
/// (when not empty) committed as `.gitattributes`.
fn crlf_fixture(attributes: &str) -> Fixture {
    let fixture = Fixture::new();
    let mut files: Vec<(&str, &[u8])> = vec![("a.txt", b"one\ntwo\n")];
    if !attributes.is_empty() {
        files.push((".gitattributes", attributes.as_bytes()));
    }
    let _ = fixture.commit("main", &files);
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.path().join(".git/config"))
        .expect("open config");
    config
        .write_all(b"[core]\n\tautocrlf = true\n")
        .expect("append config");
    fixture
}

fn side(fixture: &Fixture, form: Form) -> GitTreeSide {
    GitTreeSide::open_isolated(fixture.path(), "main", form).expect("open side")
}

/// Lists the root with `cancel` and returns each entry's path text and size.
fn listed(side: &GitTreeSide, cancel: &CancelToken) -> Result<Vec<(String, u64)>, Reason> {
    let mut out = Vec::new();
    side.list(
        &rel(""),
        &mut |entry| {
            out.push((entry.rel.as_path().to_text(), entry.size));
            ControlFlow::Continue(())
        },
        cancel,
    )?;
    Ok(out)
}

fn read_all(side: &GitTreeSide, path: &str) -> Vec<u8> {
    let mut reader = side.open(&rel(path), ReadHint::Buffered).expect("open");
    let mut out = Vec::new();
    reader.read_to_end(&mut out).expect("read");
    out
}

#[test]
fn checkout_lists_converted_size_and_reads_crlf() {
    let fixture = crlf_fixture("");
    let side = side(&fixture, Form::Checkout);
    let list = listed(&side, &CancelToken::new()).expect("list");
    assert_eq!(list, vec![("a.txt".to_owned(), 10)]);
    assert_eq!(read_all(&side, "a.txt"), b"one\r\ntwo\r\n");
}

#[test]
fn stored_keeps_stored_size_and_bytes() {
    let fixture = crlf_fixture("");
    let side = side(&fixture, Form::Stored);
    let list = listed(&side, &CancelToken::new()).expect("list");
    assert_eq!(list, vec![("a.txt".to_owned(), 8)]);
    assert_eq!(read_all(&side, "a.txt"), b"one\ntwo\n");
}

#[test]
fn stamp_and_kind_before_list_use_checkout_form() {
    let fixture = crlf_fixture("");
    let side = side(&fixture, Form::Checkout);
    assert_eq!(side.stamp(&rel("a.txt")).expect("stamp").size, 10);
    assert_eq!(side.kind(&rel("a.txt")), Ok(EntryKind::File));
}

#[test]
fn object_id_only_for_stored_form() {
    let fixture = crlf_fixture("");
    assert!(
        side(&fixture, Form::Checkout)
            .object_id(&rel("a.txt"))
            .is_none()
    );
    assert!(
        side(&fixture, Form::Stored)
            .object_id(&rel("a.txt"))
            .is_some()
    );
}

#[test]
fn minus_text_attribute_keeps_stored_size() {
    let fixture = crlf_fixture("*.txt -text\n");
    let side = side(&fixture, Form::Checkout);
    let list = listed(&side, &CancelToken::new()).expect("list");
    assert!(list.contains(&("a.txt".to_owned(), 8)), "{list:?}");
    assert_eq!(read_all(&side, "a.txt"), b"one\ntwo\n");
}

#[test]
fn cancelled_first_list_stops_and_next_list_builds() {
    let fixture = crlf_fixture("");
    let side = side(&fixture, Form::Checkout);
    let cancel = CancelToken::new();
    cancel.cancel();
    assert_eq!(listed(&side, &cancel), Err(Reason::Stopped));
    let list = listed(&side, &CancelToken::new()).expect("list");
    assert_eq!(list, vec![("a.txt".to_owned(), 10)]);
}
