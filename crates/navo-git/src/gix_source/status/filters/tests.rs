use std::fs;
use std::path::Path;

use super::{Filters, classify};

const OTHER: [&str; 1] = ["filter.crypt.clean=crypt %f"];

fn is_foreign(repo: &gix::Repository, name: &str) -> bool {
    classify(repo) == Filters::Foreign(name.to_owned())
}

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
    assert_eq!(classify(&open(dir.path(), &LFS)), Filters::Clear);
}

#[test]
fn a_defined_filter_named_by_the_worktree_attributes_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo_with(
        dir.path(),
        &LFS,
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    );
    assert_eq!(classify(&repo), Filters::Lfs);
}

#[test]
fn a_named_driver_that_is_not_the_standard_lfs_one_is_foreign() {
    for config in [
        &OTHER[..],
        &["filter.lfs.clean=my-tool clean"],
        &[
            "filter.lfs.clean=git-lfs clean -- %f",
            "filter.lfs.smudge=run-me",
        ],
        &["filter.lfs.clean=git-lfsx clean"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let attributes = "*.bin filter=lfs\n*.txt filter=crypt\n";
        let repo = repo_with(dir.path(), config, attributes);
        assert!(matches!(classify(&repo), Filters::Foreign(_)), "{config:?}");
    }
}

#[test]
fn the_foreign_name_is_the_one_the_attributes_use() {
    let dir = tempfile::tempdir().unwrap();
    let config = [LFS[0], LFS[1], LFS[2], OTHER[0]];
    let repo = repo_with(
        dir.path(),
        &config,
        "*.bin filter=lfs\n*.txt filter=crypt\n",
    );
    assert!(is_foreign(&repo, "crypt"));
}

#[test]
fn no_filter_defined_never_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo_with(dir.path(), &[], "*.bin filter=lfs\n");
    assert_eq!(classify(&repo), Filters::Clear);
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
        assert_eq!(
            classify(&repo_with(dir.path(), &LFS, attributes)),
            Filters::Clear,
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
        assert_eq!(
            classify(&repo_with(dir.path(), &LFS, attributes)),
            Filters::Lfs,
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
    assert_eq!(classify(&repo), Filters::Lfs);
}
