use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Output;

use navo_git::status::Reason;
use navo_git::trust::{Guard, untrusted_reason};
use navo_git::{CliGit, CliOptions};

use super::fixture::Fixture;
use super::trust_support::git_fixture;

const MARKER: &str = "filter-ran";

pub(super) fn fixture() -> Option<Fixture> {
    let fixture = git_fixture()?;
    fixture.init(&["-b", "main"]);
    fixture.write("a.txt", b"aaaa\n");
    fixture.write(".gitattributes", b"*.txt filter=evil\n");
    fixture.git_ok(&["add", "."]);
    fixture.commit("base");
    Some(fixture)
}

pub(super) fn append_config(fixture: &Fixture, relative: &str, text: &str) {
    let path = fixture.repo().join(".git").join(relative);
    let mut config = fs::read_to_string(&path).unwrap_or_default();
    config.push_str(text);
    fs::write(path, config).unwrap();
}

pub(super) fn status(fixture: &Fixture, worktree: &Path, args: &[OsString]) -> (Output, bool) {
    let marker = worktree.join(MARKER);
    fs::remove_file(&marker).unwrap_or_default();
    fs::write(worktree.join("a.txt"), b"bbbb\n").unwrap();
    let mut all = vec!["-C", worktree.to_str().unwrap()];
    all.extend(args.iter().map(|arg| arg.to_str().unwrap()));
    all.extend(["status", "--porcelain"]);
    let output = fixture.git(&all);
    let ran = marker.exists();
    fs::remove_file(&marker).unwrap_or_default();
    (output, ran)
}

fn status_ran_program(fixture: &Fixture, args: &[OsString]) -> bool {
    status(fixture, fixture.repo(), args).1
}

fn guard(fixture: &Fixture) -> Guard {
    Guard::read(&fixture.repo().join(".git")).unwrap()
}

/// Guarded status in `worktree` succeeds, shows the edit and runs nothing.
pub(super) fn assert_guarded_status_is_neutral(fixture: &Fixture, worktree: &Path, git_dir: &Path) {
    let guard = Guard::read(git_dir).unwrap();
    assert!(!guard.is_blocked(), "blocked: {:?}", guard.keys());
    let (output, ran) = status(fixture, worktree, &guard.args());
    assert!(!ran, "guarded status ran it");
    assert!(
        output.status.success(),
        "guarded status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), " M a.txt\n");
}

/// Plain status in `worktree` runs the filter, so the fixture does trigger it.
pub(super) fn assert_plain_status_runs_it(fixture: &Fixture, worktree: &Path) {
    assert!(
        status(fixture, worktree, &[]).1,
        "the fixture never triggers it"
    );
}

/// Guarded status is neutral in `worktree`; plain status runs the program.
pub(super) fn assert_neutralised_in(fixture: &Fixture, worktree: &Path, git_dir: &Path) {
    assert_guarded_status_is_neutral(fixture, worktree, git_dir);
    assert_plain_status_runs_it(fixture, worktree);
}

fn assert_neutralised(fixture: &Fixture) {
    assert_neutralised_in(fixture, fixture.repo(), &fixture.repo().join(".git"));
}

#[test]
fn a_clean_filter_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    append_config(
        &fixture,
        "config",
        "[filter \"evil\"]\n\tclean = touch filter-ran\n",
    );
    assert_neutralised(&fixture);
}

#[test]
fn a_required_process_filter_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    let text = "[filter \"evil\"]\n\tprocess = touch filter-ran\n\trequired = true\n";
    append_config(&fixture, "config", text);
    assert_neutralised(&fixture);
}

#[test]
fn a_filter_from_an_include_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    append_config(&fixture, "config", "[include]\n\tpath = hidden.inc\n");
    append_config(
        &fixture,
        "hidden.inc",
        "[FILTER \"evil\"]\n\tCLEAN = touch filter-ran\n",
    );
    assert_neutralised(&fixture);
}

#[test]
fn a_filter_from_config_worktree_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.git_ok(&["config", "extensions.worktreeConfig", "true"]);
    append_config(
        &fixture,
        "config.worktree",
        "[filter \"evil\"]\n\tclean = touch filter-ran\n",
    );
    assert_neutralised(&fixture);
}

#[cfg(unix)]
#[test]
fn an_fsmonitor_hook_never_runs_under_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    let marker = fixture.repo().join(MARKER);
    let hook = fixture.script(
        "fsmonitor-hook",
        &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    );
    append_config(
        &fixture,
        "config",
        &format!("[core]\n\tfsmonitor = {}\n", hook.display()),
    );
    let guard = guard(&fixture);
    assert_eq!(guard.keys(), ["core.fsmonitor"]);
    let (output, ran) = status(&fixture, fixture.repo(), &guard.args());
    assert!(!ran && output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), " M a.txt\n");
    assert!(status_ran_program(&fixture, &[]));
}

#[test]
fn a_filter_name_with_an_equals_sign_escapes_minus_c_so_it_blocks() {
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.write(".gitattributes", b"*.txt filter=a=b\n");
    append_config(
        &fixture,
        "config",
        "[filter \"a=b\"]\n\tclean = touch filter-ran\n",
    );
    let naive = ["-c", "filter.a=b.clean="].map(OsString::from);
    assert!(
        status_ran_program(&fixture, &naive),
        "-c could neutralise it after all"
    );
    assert!(guard(&fixture).is_blocked());
}

#[test]
fn a_dubious_owner_maps_to_not_trusted_and_adds_no_safe_directory() {
    let Some(fixture) = fixture() else {
        return;
    };
    let global = fixture.repo().with_file_name("global-config");
    fs::write(&global, "").unwrap();
    let config = fixture.repo().join(".git").join("config");
    let before = fs::read(&config).unwrap();
    let options = CliOptions {
        env: vec![
            env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1"),
            env("GIT_CONFIG_NOSYSTEM", "1"),
            (
                OsString::from("GIT_CONFIG_GLOBAL"),
                global.clone().into_os_string(),
            ),
        ],
        ..CliOptions::default()
    };
    let Err(error) = CliGit::with_options(fixture.repo(), &options) else {
        panic!("a dubious owner was accepted");
    };
    assert_eq!(untrusted_reason(&error), Some(Reason::NotTrusted));
    assert!(fs::read(&global).unwrap().is_empty());
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(!mentions_safe_directory(&guard(&fixture)));
}

fn mentions_safe_directory(guard: &Guard) -> bool {
    guard
        .args()
        .iter()
        .any(|arg| arg.to_string_lossy().contains("safe."))
}

fn env(key: &str, value: &str) -> (OsString, OsString) {
    (OsString::from(key), OsString::from(value))
}
