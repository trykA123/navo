//! Tests of the Git side: listing, reads, the blob cap, ids, refresh and one shared index build.

use std::io::Read;
use std::ops::ControlFlow;

use sidewise_core::ProviderPath;
use sidewise_engine::model::{CancelToken, EntryKind, ReadHint, RelPath, SideSource, Stamp};

use super::*;
use crate::fixture::Fixture;

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

fn open_side(fixture: &Fixture, rev: &str) -> GitTreeSide {
    GitTreeSide::open_isolated(fixture.path(), rev, Form::Stored).expect("open side")
}

/// Lists `folder` and returns each entry's path text, kind and size.
fn listed(side: &GitTreeSide, folder: &str) -> Result<Vec<(String, EntryKind, u64)>, Reason> {
    let mut out = Vec::new();
    side.list(
        &rel(folder),
        &mut |entry| {
            out.push((entry.rel.as_path().to_text(), entry.kind, entry.size));
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )?;
    Ok(out)
}

fn read_all(side: &GitTreeSide, path: &str) -> Result<Vec<u8>, Reason> {
    let mut reader = side.open(&rel(path), ReadHint::Buffered)?;
    let mut out = Vec::new();
    reader.read_to_end(&mut out).expect("read blob");
    Ok(out)
}

/// `main` holds `a.txt` and `src/lib.rs`; `feature` adds `b.txt` and changes `src/lib.rs`.
fn two_branches() -> Fixture {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"alpha\n"), ("src/lib.rs", b"one\n")]);
    let _ = fixture.commit(
        "feature",
        &[
            ("a.txt", b"alpha\n"),
            ("b.txt", b"beta"),
            ("src/lib.rs", b"two!\n"),
        ],
    );
    fixture
}

/// Removes the loose object file of `id`, as a pruned or partial clone would lack it.
fn remove_loose(fixture: &Fixture, id: gix::ObjectId) {
    let hex = id.to_hex().to_string();
    let loose = fixture
        .path()
        .join(".git/objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    std::fs::remove_file(loose).expect("remove the loose object");
}

#[test]
fn each_branch_lists_its_own_files_with_kinds_and_sizes() {
    let fixture = two_branches();
    let main = open_side(&fixture, "main");
    let feature = open_side(&fixture, "feature");
    assert_eq!(
        listed(&main, "").expect("main root"),
        [
            ("a.txt".to_owned(), EntryKind::File, 6),
            ("src".to_owned(), EntryKind::Folder, 0)
        ]
    );
    assert_eq!(
        listed(&feature, "").expect("feature root"),
        [
            ("a.txt".to_owned(), EntryKind::File, 6),
            ("b.txt".to_owned(), EntryKind::File, 4),
            ("src".to_owned(), EntryKind::Folder, 0)
        ]
    );
    assert_eq!(
        listed(&main, "src").expect("main src"),
        [("src/lib.rs".to_owned(), EntryKind::File, 4)]
    );
}

#[test]
fn an_unknown_folder_or_a_file_as_folder_is_not_found() {
    let fixture = two_branches();
    let side = open_side(&fixture, "main");
    assert_eq!(listed(&side, "nope").err(), Some(Reason::NotFound));
    assert_eq!(listed(&side, "a.txt").err(), Some(Reason::NotFound));
}

#[test]
fn a_breaking_sink_stops_the_listing() {
    let fixture = two_branches();
    let side = open_side(&fixture, "feature");
    let mut seen = 0;
    let fed = side.list(
        &rel(""),
        &mut |_| {
            seen += 1;
            ControlFlow::Break(())
        },
        &CancelToken::new(),
    );
    assert_eq!((fed, seen), (Ok(()), 1));
}

#[test]
fn a_cancelled_first_listing_is_stopped_and_builds_nothing() {
    let fixture = two_branches();
    let side = open_side(&fixture, "main");
    let cancel = CancelToken::new();
    cancel.cancel();
    let fed = side.list(&rel(""), &mut |_| ControlFlow::Continue(()), &cancel);
    assert_eq!(fed, Err(Reason::Stopped));
    assert_eq!(listed(&side, "").map(|rows| rows.len()), Ok(2));
}

#[test]
fn open_reads_the_stored_bytes_and_refuses_non_files() {
    let fixture = two_branches();
    let side = open_side(&fixture, "feature");
    assert_eq!(read_all(&side, "src/lib.rs"), Ok(b"two!\n".to_vec()));
    assert_eq!(read_all(&side, "src").err(), Some(Reason::Unsupported));
    assert_eq!(read_all(&side, "nope.txt").err(), Some(Reason::NotFound));
}

#[test]
fn a_blob_over_the_cap_is_unsupported() {
    let fixture = Fixture::new();
    let repo = gix::open_opts(fixture.path(), gix::open::Options::isolated()).expect("open");
    let size = MAX_BLOB + 1024 * 1024;
    let id = repo
        .write_blob_stream(std::io::repeat(b'x').take(size))
        .expect("write big blob")
        .detach();
    let mut editor = repo
        .edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))
        .expect("tree editor");
    editor
        .upsert("big.bin", gix::objs::tree::EntryKind::Blob, id)
        .expect("add big blob");
    let tree = editor.write().expect("write tree").detach();
    let signature = gix::actor::SignatureRef {
        name: "admin".into(),
        email: "admin@example.invalid".into(),
        time: "0 +0000",
    };
    repo.commit_as(
        signature,
        signature,
        "refs/heads/main",
        "big",
        tree,
        None::<gix::ObjectId>,
    )
    .expect("commit");
    let side = open_side(&fixture, "main");
    assert_eq!(
        side.stamp(&rel("big.bin")).map(|stamp| stamp.size),
        Ok(size)
    );
    assert_eq!(read_all(&side, "big.bin").err(), Some(Reason::Unsupported));
}

#[test]
fn a_blob_removed_after_the_index_is_missing_on_open() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("gone.txt", b"soon gone")]);
    let side = open_side(&fixture, "main");
    assert_eq!(listed(&side, "").map(|rows| rows.len()), Ok(1));
    let id = side.index_child_id("gone.txt").expect("indexed id");
    remove_loose(&fixture, id);
    assert_eq!(read_all(&side, "gone.txt").err(), Some(Reason::MissingBlob));
}

#[test]
fn stamp_ids_and_flags_describe_a_read_only_commit() {
    let fixture = two_branches();
    let main = open_side(&fixture, "main");
    let feature = open_side(&fixture, "feature");
    let stamp = Stamp {
        size: 6,
        modified: None,
        file_id: None,
    };
    assert_eq!(main.stamp(&rel("a.txt")), Ok(stamp));
    assert_eq!(main.stamp(&rel("nope")), Err(Reason::NotFound));
    let same = main.object_id(&rel("a.txt"));
    assert!(same.is_some());
    assert_eq!(same, feature.object_id(&rel("a.txt")));
    assert_ne!(
        main.object_id(&rel("src/lib.rs")),
        feature.object_id(&rel("src/lib.rs"))
    );
    assert_eq!(main.object_id(&rel("src")), None);
    assert_eq!(main.file_id(&rel("a.txt")), None);
    assert!(main.case_sensitive() && main.read_only() && !main.may_have_unknown_size());
    assert_eq!(main.provider().as_str(), "git");
    let hex = main
        .commit()
        .id
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        main.cache_path(&rel("src/lib.rs")).to_text(),
        format!("{hex}/src/lib.rs")
    );
}

#[test]
fn refresh_after_a_new_commit_keeps_the_pinned_commit() {
    let fixture = two_branches();
    let side = open_side(&fixture, "main");
    let before = side.commit();
    assert_eq!(listed(&side, "").map(|rows| rows.len()), Ok(2));
    let _ = fixture.commit("main", &[("a.txt", b"alpha\n"), ("new.txt", b"n")]);
    side.refresh();
    assert_eq!(
        side.commit(),
        before,
        "a side is one commit; it never moves"
    );
    let names: Vec<String> = listed(&side, "")
        .expect("root")
        .into_iter()
        .map(|row| row.0)
        .collect();
    assert_eq!(names, ["a.txt", "src"]);
    assert_ne!(side.resolve_now().expect("tip"), before);
}

#[test]
fn refresh_on_a_deleted_branch_keeps_the_old_listing() {
    let fixture = two_branches();
    let side = open_side(&fixture, "feature");
    let before = listed(&side, "").expect("root");
    std::fs::remove_file(fixture.path().join(".git/refs/heads/feature")).expect("delete branch");
    side.refresh();
    assert_eq!(listed(&side, ""), Ok(before));
    assert_eq!(read_all(&side, "b.txt"), Ok(b"beta".to_vec()));
}

#[test]
fn two_threads_listing_at_once_share_one_index_build() {
    let fixture = two_branches();
    let side = open_side(&fixture, "feature");
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(|| listed(&side, ""));
        let right = scope.spawn(|| listed(&side, ""));
        (left.join().expect("left"), right.join().expect("right"))
    });
    assert_eq!(left, right);
    assert_eq!(left.map(|rows| rows.len()), Ok(3));
    assert_eq!(side.builds.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn a_missing_revision_or_repository_does_not_open() {
    let fixture = two_branches();
    let missing = GitTreeSide::open_isolated(fixture.path(), "nope", Form::Stored);
    assert_eq!(missing.err(), Some(GitProblem::NoSuchRevision));
    let empty = tempfile::tempdir().expect("temp");
    let none = GitTreeSide::open_isolated(empty.path(), "main", Form::Stored);
    assert_eq!(none.err(), Some(GitProblem::NotRepository));
}

#[test]
fn reads_before_any_listing_look_up_one_path_and_build_no_index() {
    let fixture = two_branches();
    let side = open_side(&fixture, "main");
    assert_eq!(read_all(&side, "src/lib.rs"), Ok(b"one\n".to_vec()));
    assert_eq!(side.stamp(&rel("a.txt")).map(|stamp| stamp.size), Ok(6));
    assert!(side.object_id(&rel("a.txt")).is_some());
    assert!(
        side.object_id(&rel("src")).is_none(),
        "a folder has no blob id"
    );
    assert_eq!(side.stamp(&rel("nope")).err(), Some(Reason::NotFound));
    assert_eq!(side.stamp(&rel("a.txt/x")).err(), Some(Reason::NotFound));
    assert_eq!(
        side.open(&rel("src"), ReadHint::Buffered).err(),
        Some(Reason::Unsupported)
    );
    assert_eq!(side.builds.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(
        side.stamp(&rel("../a.txt")).err(),
        Some(Reason::Unsupported)
    );
}
