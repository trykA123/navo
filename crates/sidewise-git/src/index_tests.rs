//! Tests of the tree index: kinds, sizes, lookups, cancel and the depth and path caps.

use gix::objs::tree::EntryKind as GitKind;

use super::*;
use crate::fixture::Fixture;

/// A commit whose tree holds `entries` (path, kind, object id) on `refs/heads/main`.
fn commit_entries(fixture: &Fixture, entries: &[(&[u8], GitKind, gix::ObjectId)]) -> Commit {
    let repo = gix::open_opts(fixture.path(), gix::open::Options::isolated()).expect("open");
    let mut editor = repo
        .edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))
        .expect("tree editor");
    for (path, kind, id) in entries {
        let path: &gix::bstr::BStr = (*path).into();
        editor.upsert(path, *kind, *id).expect("add tree entry");
    }
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
        "test",
        tree,
        None::<gix::ObjectId>,
    )
    .expect("write commit");
    open(fixture).resolve("main").expect("resolve main")
}

/// Writes `bytes` as a blob in the fixture repository.
fn blob(fixture: &Fixture, bytes: &[u8]) -> gix::ObjectId {
    open(fixture)
        .local()
        .write_blob(bytes)
        .expect("write blob")
        .detach()
}

fn open(fixture: &Fixture) -> Repo {
    Repo::open_isolated(fixture.path()).expect("open fixture")
}

/// Builds the index of `main` with a fresh token.
fn build_main(fixture: &Fixture) -> Result<TreeIndex, Reason> {
    let repo = open(fixture);
    let commit = repo.resolve("main").expect("resolve main");
    TreeIndex::build(&repo, commit, &CancelToken::new())
}

fn names(children: &[Child]) -> Vec<&str> {
    children.iter().map(|child| child.name.as_str()).collect()
}

/// A fixture with `a.txt`, `src/lib.rs` and `src/deep/x.rs`.
fn three_files() -> Fixture {
    let fixture = Fixture::new();
    let _ = fixture.commit(
        "main",
        &[
            ("a.txt", b"alpha\n"),
            ("src/lib.rs", b"pub fn f() {}\n"),
            ("src/deep/x.rs", b"x"),
        ],
    );
    fixture
}

#[test]
fn every_folder_lists_its_children_sorted_with_kinds_and_sizes() {
    let fixture = three_files();
    let index = build_main(&fixture).expect("build");
    let root = index.children("").expect("root");
    assert_eq!(names(root), ["a.txt", "src"]);
    assert_eq!((root[0].kind, root[0].size), (EntryKind::File, 6));
    assert_eq!(root[1].kind, EntryKind::Folder);
    let src = index.children("src").expect("src");
    assert_eq!(names(src), ["deep", "lib.rs"]);
    assert_eq!(src[1].size, 14);
    assert_eq!(names(index.children("src/deep").expect("deep")), ["x.rs"]);
    assert!(
        root.iter().all(|child| child.readable),
        "UTF-8 names are readable"
    );
    assert!(
        index.children("nope").is_none(),
        "an unknown folder has no listing"
    );
}

#[test]
fn child_finds_a_nested_file_and_not_a_missing_one() {
    let fixture = three_files();
    let index = build_main(&fixture).expect("build");
    let lib = index.child("src/lib.rs").expect("src/lib.rs");
    assert_eq!((lib.name.as_str(), lib.kind), ("lib.rs", EntryKind::File));
    assert_eq!(index.child("a.txt").map(|child| child.size), Some(6));
    assert!(index.child("nope").is_none());
    assert!(index.child("src/nope").is_none());
}

#[test]
fn a_cancelled_token_stops_the_build() {
    let fixture = three_files();
    let repo = open(&fixture);
    let cancel = CancelToken::new();
    cancel.cancel();
    let commit = repo.resolve("main").expect("resolve");
    assert_eq!(
        TreeIndex::build(&repo, commit, &cancel).err(),
        Some(Reason::Stopped)
    );
}

#[test]
fn a_symlink_is_a_link_with_its_target_size_and_a_gitlink_a_submodule() {
    let fixture = Fixture::new();
    let target = blob(&fixture, b"a.txt");
    let other = gix::ObjectId::from_hex(b"0123456789abcdef0123456789abcdef01234567").expect("hex");
    let _ = commit_entries(
        &fixture,
        &[
            (&b"link"[..], GitKind::Link, target),
            (&b"sub"[..], GitKind::Commit, other),
        ],
    );
    let index = build_main(&fixture).expect("build");
    let link = index.child("link").expect("link");
    assert_eq!((link.kind, link.size), (EntryKind::Link, 5));
    let sub = index.child("sub").expect("sub");
    assert_eq!(
        (sub.kind, sub.size, sub.id),
        (EntryKind::Submodule, 0, other)
    );
}

#[test]
fn a_missing_blob_fails_the_build_as_missing() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"soon gone");
    let _ = commit_entries(&fixture, &[(&b"gone.txt"[..], GitKind::Blob, id)]);
    let hex = id.to_hex().to_string();
    let loose = fixture
        .path()
        .join(".git/objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    std::fs::remove_file(loose).expect("remove the loose blob");
    assert_eq!(build_main(&fixture).err(), Some(Reason::MissingBlob));
}

#[test]
fn a_file_at_the_depth_cap_builds_and_one_deeper_is_unsupported() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"deep");
    let at_cap = format!("{}f", "d/".repeat(MAX_DEPTH - 1));
    let _ = commit_entries(&fixture, &[(at_cap.as_bytes(), GitKind::Blob, id)]);
    let index = build_main(&fixture).expect("128 components build");
    assert_eq!(index.child(&at_cap).map(|child| child.size), Some(4));
    let deeper = Fixture::new();
    let id = blob(&deeper, b"deep");
    let too_deep = format!("{}f", "d/".repeat(MAX_DEPTH));
    let _ = commit_entries(&deeper, &[(too_deep.as_bytes(), GitKind::Blob, id)]);
    assert_eq!(build_main(&deeper).err(), Some(Reason::Unsupported));
}

#[test]
fn a_name_longer_than_the_path_cap_is_unsupported() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"long");
    let name = "n".repeat(17 * 1024);
    let _ = commit_entries(&fixture, &[(name.as_bytes(), GitKind::Blob, id)]);
    assert_eq!(build_main(&fixture).err(), Some(Reason::Unsupported));
}

#[test]
fn a_non_utf8_name_is_listed_lossy_and_unreadable() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"bytes");
    let _ = commit_entries(&fixture, &[(&b"bad\xff/in.txt"[..], GitKind::Blob, id)]);
    let index = build_main(&fixture).expect("build");
    let folder = index.child("bad\u{fffd}").expect("lossy folder");
    assert!(
        !folder.readable,
        "a non-UTF-8 folder cannot be read by name"
    );
    let inner = index.child("bad\u{fffd}/in.txt").expect("lossy child");
    assert!(
        !inner.readable,
        "a path through a non-UTF-8 folder cannot be read"
    );
}

/// A commit whose root tree is written raw with one entry `name`, which `edit_tree` refuses.
fn commit_raw_entry(fixture: &Fixture, name: &[u8]) -> Commit {
    let id = blob(fixture, b"raw");
    commit_raw_tree(fixture, &[name], id)
}

/// A commit whose root tree is written raw with one blob entry `id` per name, unchecked.
fn commit_raw_tree(fixture: &Fixture, names: &[&[u8]], id: gix::ObjectId) -> Commit {
    let repo = gix::open_opts(fixture.path(), gix::open::Options::isolated()).expect("open");
    let entries = names.iter().map(|name| gix::objs::tree::Entry {
        mode: GitKind::Blob.into(),
        filename: (*name).into(),
        oid: id,
    });
    let tree = gix::objs::Tree {
        entries: entries.collect(),
    };
    let tree = repo.write_object(&tree).expect("write raw tree").detach();
    let signature = gix::actor::SignatureRef {
        name: "admin".into(),
        email: "admin@example.invalid".into(),
        time: "0 +0000",
    };
    let _ = repo
        .commit_as(
            signature,
            signature,
            "refs/heads/main",
            "raw",
            tree,
            None::<gix::ObjectId>,
        )
        .expect("write commit");
    open(fixture).resolve("main").expect("resolve main")
}

#[test]
fn a_dot_dot_or_slash_name_in_a_raw_tree_is_unsupported() {
    for name in [&b".."[..], b"a/b", b"."] {
        let fixture = Fixture::new();
        let _ = commit_raw_entry(&fixture, name);
        assert_eq!(
            build_main(&fixture).err(),
            Some(Reason::Unsupported),
            "{:?} must be refused",
            gix::bstr::BStr::new(name)
        );
    }
}

#[test]
fn two_sibling_names_equal_after_the_lossy_conversion_are_unsupported() {
    let fixture = Fixture::new();
    let one = blob(&fixture, b"one");
    let two = blob(&fixture, b"two");
    let _ = commit_entries(
        &fixture,
        &[
            (&b"a\xff"[..], GitKind::Blob, one),
            (&b"a\xfe"[..], GitKind::Blob, two),
        ],
    );
    assert_eq!(build_main(&fixture).err(), Some(Reason::Unsupported));
    let folders = Fixture::new();
    let one = blob(&folders, b"one");
    let _ = commit_entries(
        &folders,
        &[
            (&b"p\xff/x"[..], GitKind::Blob, one),
            (&b"p\xfe/y"[..], GitKind::Blob, one),
        ],
    );
    assert_eq!(build_main(&folders).err(), Some(Reason::Unsupported));
}

#[test]
fn the_byte_budget_counts_folder_keys_and_names() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"x");
    let folder = "d".repeat(40);
    let _ = commit_entries(
        &fixture,
        &[(format!("{folder}/f").as_bytes(), GitKind::Blob, id)],
    );
    let repo = open(&fixture);
    let commit = repo.resolve("main").expect("resolve main");
    // Names cost 41 bytes and the folder key 40: 60 fits the names only.
    let over = build_within(&repo, commit.clone(), &CancelToken::new(), 60);
    assert_eq!(over.err(), Some(Reason::Unsupported));
    let exact = build_within(&repo, commit, &CancelToken::new(), 81);
    assert!(exact.is_ok(), "81 bytes hold every key and name");
}

/// Looks up `rel` in the tree of `main` without an index.
fn lookup_main(fixture: &Fixture, rel: &str) -> Result<Option<Child>, Reason> {
    let repo = open(fixture);
    let commit = repo.resolve("main").expect("resolve main");
    lookup(&repo, commit.tree, rel)
}

/// Removes the loose object `id` from the fixture repository.
fn remove_loose(fixture: &Fixture, id: gix::ObjectId) {
    let hex = id.to_hex().to_string();
    let loose = fixture.path().join(".git/objects").join(&hex[..2]);
    std::fs::remove_file(loose.join(&hex[2..])).expect("remove the loose object");
}

#[test]
fn lookup_finds_a_nested_file_and_not_a_missing_one() {
    let fixture = three_files();
    let lib = lookup_main(&fixture, "src/deep/x.rs").expect("lookup");
    assert_eq!(lib.map(|child| child.kind), Some(EntryKind::File));
    assert!(lookup_main(&fixture, "src/nope").expect("lookup").is_none());
}

#[test]
fn lookup_through_a_submodule_or_a_file_is_not_found_without_loading_it() {
    let fixture = Fixture::new();
    let file = blob(&fixture, b"a file");
    let other = gix::ObjectId::from_hex(b"0123456789abcdef0123456789abcdef01234567").expect("hex");
    let _ = commit_entries(
        &fixture,
        &[
            (&b"f.txt"[..], GitKind::Blob, file),
            (&b"sub"[..], GitKind::Commit, other),
        ],
    );
    remove_loose(&fixture, file);
    assert_eq!(
        lookup_main(&fixture, "sub/x").map(|c| c.is_none()),
        Ok(true)
    );
    assert_eq!(
        lookup_main(&fixture, "f.txt/x").map(|c| c.is_none()),
        Ok(true)
    );
}

#[test]
fn lookup_through_a_damaged_tree_is_a_read_error() {
    let fixture = Fixture::new();
    let repo = gix::open_opts(fixture.path(), gix::open::Options::isolated()).expect("open");
    let damaged = gix::objs::Write::write_buf(
        &repo.objects,
        gix::object::Kind::Tree,
        b"100644 no-nul-or-id",
    )
    .expect("write damaged tree");
    let _ = commit_entries(&fixture, &[(&b"d"[..], GitKind::Tree, damaged)]);
    assert_eq!(lookup_main(&fixture, "d/x").err(), Some(Reason::ReadError));
}

#[test]
fn lookup_refuses_duplicate_names_like_the_index() {
    let fixture = Fixture::new();
    let id = blob(&fixture, b"dup");
    let _ = commit_raw_tree(&fixture, &[b"a", b"a"], id);
    assert_eq!(build_main(&fixture).err(), Some(Reason::Unsupported));
    assert_eq!(lookup_main(&fixture, "a").err(), Some(Reason::Unsupported));
    let lossy = Fixture::new();
    let id = blob(&lossy, b"dup");
    let _ = commit_raw_tree(&lossy, &[b"a\xef\xbf\xbd", b"a\xff"], id);
    assert_eq!(
        lookup_main(&lossy, "a\u{fffd}").err(),
        Some(Reason::Unsupported)
    );
}
