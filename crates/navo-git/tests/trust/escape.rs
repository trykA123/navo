use std::fs;

use navo_git::trust::{Guard, UNREADABLE_KEY};

use super::trust_git::{
    append_config, assert_guarded_status_is_neutral, assert_neutralised_in, fixture,
};
use super::trust_support::{GitDir, neutralised};

#[test]
fn a_dotted_filter_section_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.write(".gitattributes", b"*.txt filter=a.b\n");
    fixture.git_ok(&["commit", "-q", "-a", "-m", "dotted"]);
    append_config(
        &fixture,
        "config",
        "[filter.a \"b\"]\n\tclean = touch filter-ran\n",
    );
    let git_dir = fixture.repo().join(".git");
    assert_neutralised_in(&fixture, fixture.repo(), &git_dir);
}

#[test]
fn dotted_section_names_flatten_as_git_does() {
    let guard = GitDir::new(
        "[Filter.A \"b\"]\n\tclean = x\n[filter.C.D]\n\tsmudge = x\n\
         [remote.O \"r\"]\n\tuploadPack = x\n",
    )
    .guard();
    assert!(neutralised(&guard, "a.b"));
    assert!(neutralised(&guard, "c.d"));
    assert_eq!(
        guard.keys(),
        [
            "filter.a.b.clean",
            "filter.c.d.smudge",
            "remote.o.r.uploadpack"
        ]
    );
}

#[test]
fn a_commondir_with_extra_line_endings_still_finds_the_common_config() {
    let Some(fixture) = fixture() else {
        return;
    };
    let worktree = fixture.repo().with_file_name("linked");
    fixture.git_ok(&["worktree", "add", "-q", "../linked"]);
    let git_dir = fixture.repo().join(".git").join("worktrees").join("linked");
    let commondir = git_dir.join("commondir");
    assert_eq!(fs::read(&commondir).unwrap(), b"../..\n");
    fs::write(&commondir, b"../..\r\n\n\r\n").unwrap();
    append_config(
        &fixture,
        "config",
        "[filter \"evil\"]\n\tclean = touch filter-ran\n",
    );
    let guard = Guard::read(&git_dir).unwrap();
    assert_eq!(guard.keys(), ["filter.evil.clean"]);
    assert_guarded_status_is_neutral(&fixture, &worktree, &git_dir);
    // On Windows git did not run the filter through this commondir, so only the guard is checked.
    #[cfg(unix)]
    super::trust_git::assert_plain_status_runs_it(&fixture, &worktree);
}

#[test]
fn a_commondir_that_does_not_resolve_blocks() {
    let repo = GitDir::new("");
    for target in [&b"missing\n"[..], b"\n\r\n", b"config\n"] {
        fs::write(repo.git_dir.join("commondir"), target).unwrap();
        let guard = Guard::read_with_home(&repo.git_dir, Some(&repo.home())).unwrap();
        assert!(guard.is_blocked(), "{target:?}");
        assert_eq!(guard.keys(), [UNREADABLE_KEY]);
    }
}
