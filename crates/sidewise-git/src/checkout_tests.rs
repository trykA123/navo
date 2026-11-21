//! Tests of the checkout converter: line endings, ident, binaries, attributes, a bare
//! repository, the system config, the network guard, and that no filter driver ever runs.

use super::*;
use crate::fixture::Fixture;
use crate::index::Child;
use crate::repo::{GitProblem, Repo};
use sidewise_engine::model::EntryKind;
use std::io::Write;

const fn assert_send<T: Send>() {}
const _: () = assert_send::<Checkout>();

/// A fixture with `files` committed, `attributes` (when not empty) committed as `.gitattributes`
/// and `config` appended to `.git/config`; returns it with the opened repository and the tree id.
fn setup(
    files: &[(&str, &[u8])],
    attributes: &str,
    config: &str,
) -> (Fixture, gix::Repository, gix::ObjectId) {
    let fixture = Fixture::new();
    let mut all = files.to_vec();
    if !attributes.is_empty() {
        all.push((".gitattributes", attributes.as_bytes()));
    }
    let commit = fixture.commit("main", &all);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.path().join(".git/config"))
        .expect("open config");
    file.write_all(config.as_bytes()).expect("append config");
    let repo = Repo::open_isolated(fixture.path()).expect("open").local();
    let tree = repo
        .find_commit(commit)
        .expect("commit")
        .tree_id()
        .expect("tree")
        .detach();
    (fixture, repo, tree)
}

const AUTOCRLF: &str = "[core]\n\tautocrlf = true\n";

/// A readable file child of `size` bytes with the blob id of `data`.
fn file_child(name: &str, data: &[u8]) -> Child {
    Child {
        name: name.to_owned(),
        kind: EntryKind::File,
        size: data.len() as u64,
        id: gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, data)
            .expect("hash"),
        readable: true,
    }
}

/// Runs `fix_child` with a `read` that returns `data` and counts its calls.
fn fix(checkout: &mut Checkout, repo: &gix::Repository, child: &mut Child, data: &[u8]) -> usize {
    let mut count = 0;
    let name = child.name.clone();
    checkout.fix_child(repo, &name, child, &mut |_| {
        count += 1;
        Ok(data.to_vec())
    });
    count
}

#[test]
fn a_plain_file_is_never_read() {
    let (_f, repo, tree) = setup(&[("a.txt", b"one\ntwo\n")], "", "");
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let mut child = file_child("a.txt", b"one\ntwo\n");
    assert_eq!(fix(&mut checkout, &repo, &mut child, b"one\ntwo\n"), 0);
    assert_eq!(child.size, 8);
    assert!(child.readable);
}

#[test]
fn autocrlf_turns_lf_into_crlf() {
    let (_f, repo, tree) = setup(&[("a.txt", b"one\ntwo\n")], "", AUTOCRLF);
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let out = checkout
        .convert(&repo, "a.txt", b"one\ntwo\n")
        .expect("convert");
    assert_eq!(out, b"one\r\ntwo\r\n");
    let mut child = file_child("a.txt", b"one\ntwo\n");
    assert_eq!(fix(&mut checkout, &repo, &mut child, b"one\ntwo\n"), 1);
    assert_eq!(child.size, 10);
}

#[test]
fn a_binary_is_unchanged_under_autocrlf() {
    let (_f, repo, tree) = setup(&[("b.bin", b"x\0y\n")], "", AUTOCRLF);
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let out = checkout
        .convert(&repo, "b.bin", b"x\0y\n")
        .expect("convert");
    assert_eq!(out, b"x\0y\n");
}

#[test]
fn minus_text_keeps_lf_under_autocrlf() {
    let (_f, repo, tree) = setup(&[("c.md", b"a\nb\n")], "*.md -text\n", AUTOCRLF);
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let out = checkout.convert(&repo, "c.md", b"a\nb\n").expect("convert");
    assert_eq!(out, b"a\nb\n");
    let mut child = file_child("c.md", b"a\nb\n");
    let _ = fix(&mut checkout, &repo, &mut child, b"a\nb\n");
    assert_eq!(child.size, 4);
}

#[test]
fn ident_expands_the_blob_id() {
    let data = b"v $Id$\n";
    let (_f, repo, tree) = setup(&[("a.txt", data)], "a.txt ident\n", "");
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let out = checkout.convert(&repo, "a.txt", data).expect("convert");
    let id =
        gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, data).expect("hash");
    assert_eq!(out, format!("v $Id: {id}$\n").into_bytes());
    let mut child = file_child("a.txt", data);
    assert_eq!(fix(&mut checkout, &repo, &mut child, data), 1);
    assert_eq!(child.size, out.len() as u64);
}

#[test]
fn a_large_ident_file_is_not_read() {
    let (_f, repo, tree) = setup(&[("a.txt", b"x")], "a.txt ident\n", "");
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let mut child = file_child("a.txt", b"x");
    child.size = 2 * 1024 * 1024;
    assert_eq!(fix(&mut checkout, &repo, &mut child, b"x"), 0);
    assert!(!child.readable);
}

/// Writes a shell script that creates `marker` and passes its input through.
fn marker_script(dir: &std::path::Path, marker: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("smudge.sh");
    let text = format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display());
    std::fs::write(&script, text).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }
    script
}

#[test]
fn no_filter_driver_runs() {
    let tmp = tempfile::tempdir().expect("temp");
    let marker = tmp.path().join("marker");
    let script = marker_script(tmp.path(), &marker);
    // Git config treats `\` as an escape, so a Windows path is quoted with its `\` doubled.
    let path = script.display().to_string().replace('\\', "\\\\");
    let config = format!("{AUTOCRLF}[filter \"x\"]\n\tsmudge = \"{path}\"\n");
    let (_f, repo, tree) = setup(&[("a.txt", b"one\n")], "*.txt filter=x\n", &config);
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    let out = checkout.convert(&repo, "a.txt", b"one\n");
    assert!(!marker.exists(), "a filter driver ran");
    assert_eq!(out.expect("convert"), b"one\r\n");
}

#[test]
fn a_bare_repository_takes_attributes_from_the_tree() {
    let dir = tempfile::tempdir().expect("temp");
    let repo = gix::ThreadSafeRepository::init_opts(
        dir.path(),
        gix::create::Kind::Bare,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .expect("init bare")
    .to_thread_local();
    let mut editor = repo
        .edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))
        .expect("editor");
    for (path, bytes) in [
        (".gitattributes", &b"*.txt text eol=crlf\n"[..]),
        ("d.txt", b"l\n"),
    ] {
        let blob = repo.write_blob(bytes).expect("blob").detach();
        editor
            .upsert(path, gix::objs::tree::EntryKind::Blob, blob)
            .expect("upsert");
    }
    let tree = editor.write().expect("tree").detach();
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    assert_eq!(
        checkout.convert(&repo, "d.txt", b"l\n").expect("convert"),
        b"l\r\n"
    );
}

#[test]
fn the_system_config_turns_on_autocrlf() {
    let (_f, repo, tree) = setup(&[("a.txt", b"one\n")], "", "");
    let system = gix::config::File::try_from(AUTOCRLF).expect("system");
    let mut checkout = Checkout::open(&repo, tree, Some(&system)).expect("open");
    assert_eq!(
        checkout.convert(&repo, "a.txt", b"one\n").expect("convert"),
        b"one\r\n"
    );
}

#[test]
fn a_network_attributes_file_refuses_to_open() {
    let (_f, repo, tree) = setup(
        &[("a.txt", b"one\n")],
        "",
        "[core]\n\tattributesFile = //host/x\n",
    );
    assert_eq!(
        Checkout::open(&repo, tree, None).err(),
        Some(GitProblem::Unsupported)
    );
}

#[test]
fn uncommitted_working_tree_attributes_are_ignored() {
    let (f, repo, tree) = setup(&[("a.txt", b"l\n")], "", "");
    std::fs::write(f.path().join(".gitattributes"), "* text eol=crlf\n").expect("attributes");
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    assert_eq!(
        checkout.convert(&repo, "a.txt", b"l\n").expect("convert"),
        b"l\n"
    );
}

#[test]
fn committed_attributes_apply_without_the_working_file() {
    let (f, repo, tree) = setup(&[("a.txt", b"l\n")], "*.txt text eol=crlf\n", "");
    assert!(!f.path().join(".gitattributes").exists());
    let mut checkout = Checkout::open(&repo, tree, None).expect("open");
    assert_eq!(
        checkout.convert(&repo, "a.txt", b"l\n").expect("convert"),
        b"l\r\n"
    );
}
