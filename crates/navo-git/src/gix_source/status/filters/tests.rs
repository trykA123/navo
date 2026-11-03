use std::fs;
use std::path::Path;

use super::refused;

const LFS: [&str; 3] = [
    "filter.lfs.clean=git-lfs clean -- %f",
    "filter.lfs.smudge=git-lfs smudge -- %f",
    "filter.lfs.process=git-lfs filter-process",
];

fn open(dir: &Path, config: &[&str]) -> gix::Repository {
    gix::init(dir).unwrap();
    gix::open::Options::isolated()
        .config_overrides(config.iter().copied())
        .open(dir)
        .unwrap()
        .to_thread_local()
}

fn repo_with(dir: &Path, config: &[&str], attributes: &str) -> gix::Repository {
    fs::write(dir.join(".gitattributes"), attributes).unwrap();
    open(dir, config)
}

#[test]
fn a_defined_filter_that_nothing_uses_does_not_refuse() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!refused(&open(dir.path(), &LFS)));
}

#[test]
fn a_defined_filter_named_by_the_worktree_attributes_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo_with(
        dir.path(),
        &LFS,
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    );
    assert!(refused(&repo));
}

#[test]
fn no_filter_defined_never_refuses() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!refused(&repo_with(dir.path(), &[], "*.bin filter=lfs\n")));
}

#[test]
fn another_filter_name_or_a_cleared_one_does_not_refuse() {
    for attributes in [
        "*.bin filter=other\n",
        "*.bin -filter\n",
        "*.bin !filter\n",
        "# *.bin filter=lfs\n",
        "*.bin diff=lfs\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            !refused(&repo_with(dir.path(), &LFS, attributes)),
            "{attributes}"
        );
    }
}

#[test]
fn a_macro_or_a_quoted_pattern_that_sets_the_filter_refuses() {
    for attributes in [
        "[attr]big filter=lfs\n",
        "\"my file.bin\" text filter=lfs\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            refused(&repo_with(dir.path(), &LFS, attributes)),
            "{attributes}"
        );
    }
}

#[test]
fn the_info_attributes_file_counts() {
    let dir = tempfile::tempdir().unwrap();
    let repo = open(dir.path(), &LFS);
    let info = repo.common_dir().join("info");
    fs::create_dir_all(&info).unwrap();
    fs::write(info.join("attributes"), "*.psd filter=lfs\n").unwrap();
    assert!(refused(&repo));
}
