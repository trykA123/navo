//! Characterization tests: a remote-tracking ref such as `origin/main` works as a Git side.

use std::io::Read;

use sidewise_core::ProviderPath;
use sidewise_engine::model::{ReadHint, RelPath, SideSource};

use crate::fixture::Fixture;
use crate::location::Form;
use crate::repo::Repo;
use crate::side::GitTreeSide;

/// Builds a repository whose local `main` and `refs/remotes/origin/main` hold different commits.
/// Returns the fixture, the local commit id and the remote commit id.
fn with_remote_branch() -> (Fixture, gix::ObjectId, gix::ObjectId) {
    let fixture = Fixture::new();
    let local = fixture.commit("main", &[("a.txt", b"local\n")]);
    let remote = fixture.commit("fetched", &[("a.txt", b"remote\n")]);
    let ref_file = fixture.path().join(".git/refs/remotes/origin/main");
    std::fs::create_dir_all(ref_file.parent().expect("parent")).expect("make ref folder");
    std::fs::write(ref_file, format!("{remote}\n")).expect("write remote-tracking ref");
    (fixture, local, remote)
}

/// `origin/main` resolves to the fetched commit and `main` stays on the local one.
#[test]
fn origin_main_resolves_the_remote_tracking_ref() {
    let (fixture, local, remote) = with_remote_branch();
    let repo = Repo::open_isolated(fixture.path()).expect("open repo");
    let origin = repo.resolve("origin/main").expect("resolve origin/main");
    let main = repo.resolve("main").expect("resolve main");
    assert_eq!(origin.short, remote.to_string()[..7]);
    assert_eq!(main.short, local.to_string()[..7]);
}

/// A side at `origin/main` reads the file content of the remote commit.
#[test]
fn tree_at_origin_main_shows_remote_content() {
    let (fixture, _, _) = with_remote_branch();
    let read = |rev: &str| {
        let side = GitTreeSide::open_isolated(fixture.path(), rev, Form::Stored).expect("side");
        let rel = RelPath::new(ProviderPath::from_text("a.txt"));
        let mut out = String::new();
        side.open(&rel, ReadHint::Buffered)
            .expect("open file")
            .read_to_string(&mut out)
            .expect("read file");
        out
    };
    assert_eq!(read("origin/main"), "remote\n");
    assert_eq!(read("main"), "local\n");
}
