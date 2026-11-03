use std::fs;

use navo_git::status::{GitStatus, RepoStatus};
use navo_git::{Cancel, GitError, GixGit};

use super::fixture::Fixture;
use super::gix_status_support::{commit_file, repo};

/// Defines the LFS filter in a user config, the way Git for Windows does at the system level.
/// Set after the fixture's own git runs. Each test is its own process under nextest.
fn define_lfs_filter(fixture: &Fixture) {
    define_filter(
        fixture,
        "[filter \"lfs\"]\n\tclean = git-lfs clean -- %f\n\tsmudge = git-lfs smudge -- %f\n\
         \tprocess = git-lfs filter-process\n\trequired = true\n",
    );
}

fn define_filter(fixture: &Fixture, text: &str) {
    let config = fixture.repo().with_file_name("global.gitconfig");
    fs::write(&config, text).unwrap();
    // SAFETY: the test process runs one test and spawns no thread that reads the environment.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
}

fn status(fixture: &Fixture) -> Result<RepoStatus, GitError> {
    let gix = GixGit::new(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    gix.status(&guard, &Cancel::default())
}

#[test]
fn a_user_lfs_filter_that_no_attribute_names_does_not_stop_the_status() {
    let Some(fixture) = repo() else { return };
    fixture.write("new.txt", b"new\n");
    define_lfs_filter(&fixture);
    let status = status(&fixture).unwrap();
    assert_eq!(status.untracked, 1);
}

#[test]
fn a_user_filter_named_by_the_attributes_is_refused_by_name() {
    let Some(fixture) = repo() else { return };
    fixture.write(
        ".gitattributes",
        b"*.bin filter=lfs diff=lfs merge=lfs -text\n",
    );
    define_filter(&fixture, "[filter \"lfs\"]\n\tclean = convert %f\n");
    assert!(matches!(
        status(&fixture),
        Err(GitError::FilterRefused { ref filter, .. }) if filter == "lfs"
    ));
}

#[test]
fn a_filter_named_by_a_tracked_nested_attributes_file_is_refused() {
    let Some(fixture) = repo() else { return };
    fs::create_dir(fixture.repo().join("art")).unwrap();
    commit_file(&fixture, "art/.gitattributes", "*.psd filter=lfs\n", "art");
    define_filter(&fixture, "[filter \"lfs\"]\n\tclean = convert %f\n");
    assert!(matches!(
        status(&fixture),
        Err(GitError::FilterRefused { .. })
    ));
}
