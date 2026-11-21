//! Tests of the checkout settings: the system config path, its reader, the eol settings and the
//! attributes-file guard.

use super::*;
use crate::fixture::Fixture;
use crate::repo::{GitProblem, Repo};
use gix::filter::plumbing::eol::{AutoCrlf, Mode};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

fn env(pairs: &[(&str, &str)]) -> HashMap<String, OsString> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect()
}

fn path_for(vars: &HashMap<String, OsString>, windows: bool, existing: &[&str]) -> Option<PathBuf> {
    let set: HashSet<PathBuf> = existing.iter().map(PathBuf::from).collect();
    system_config_path(&|key| vars.get(key).cloned(), windows, &|path| {
        set.contains(path)
    })
}

#[test]
fn nosystem_wins_over_an_explicit_system_path() {
    let vars = env(&[
        ("GIT_CONFIG_NOSYSTEM", " Yes "),
        ("GIT_CONFIG_SYSTEM", "/x/c"),
    ]);
    assert_eq!(path_for(&vars, false, &["/etc/gitconfig"]), None);
}

#[test]
fn nosystem_off_does_not_count() {
    for off in ["off", "0", " 0 "] {
        let vars = env(&[("GIT_CONFIG_NOSYSTEM", off), ("GIT_CONFIG_SYSTEM", "/x/c")]);
        assert_eq!(
            path_for(&vars, false, &[]),
            Some(PathBuf::from("/x/c")),
            "{off}"
        );
    }
}

#[test]
fn nosystem_any_nonzero_integer_is_true() {
    for on in ["1", "2", "-1", " 42 "] {
        let vars = env(&[("GIT_CONFIG_NOSYSTEM", on), ("GIT_CONFIG_SYSTEM", "/x/c")]);
        assert_eq!(path_for(&vars, false, &[]), None, "{on}");
    }
}

#[test]
fn a_network_windows_candidate_is_never_checked() {
    let vars = env(&[("LOCALAPPDATA", r"\\host\share")]);
    let found = system_config_path(&|key| vars.get(key).cloned(), true, &|path| {
        assert!(
            !path.to_string_lossy().starts_with(r"\\"),
            "stat of {path:?}"
        );
        true
    });
    assert_eq!(found, None);
}

#[test]
fn an_empty_system_variable_is_ignored() {
    let vars = env(&[("GIT_CONFIG_SYSTEM", "")]);
    assert_eq!(
        path_for(&vars, false, &["/etc/gitconfig"]),
        Some(PathBuf::from("/etc/gitconfig"))
    );
}

#[test]
fn the_first_existing_windows_candidate_wins() {
    let vars = env(&[
        ("ProgramFiles", r"C:\PF"),
        ("ProgramFiles(x86)", r"C:\PF86"),
        ("LOCALAPPDATA", r"C:\L"),
    ]);
    let second = Path::new(r"C:\PF86")
        .join("Git")
        .join("etc")
        .join("gitconfig");
    let third = Path::new(r"C:\L")
        .join("Programs")
        .join("Git")
        .join("etc")
        .join("gitconfig");
    let existing = [
        second.to_str().expect("utf8"),
        third.to_str().expect("utf8"),
    ];
    assert_eq!(path_for(&vars, true, &existing), Some(second.clone()));
    assert_eq!(path_for(&vars, true, &[]), None);
}

#[test]
fn a_unix_machine_uses_etc_gitconfig_when_present() {
    let vars = env(&[]);
    assert_eq!(
        path_for(&vars, false, &["/etc/gitconfig"]),
        Some(PathBuf::from("/etc/gitconfig"))
    );
    assert_eq!(path_for(&vars, false, &[]), None);
}

#[test]
fn read_system_refuses_a_file_over_one_mebibyte() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("gitconfig");
    let mut file = std::fs::File::create(&path).expect("create");
    file.write_all(&vec![b'#'; MAX_SYSTEM_CONFIG as usize + 1])
        .expect("write");
    assert!(read_system(&path).is_none());
}

#[test]
fn read_system_refuses_a_network_path() {
    assert!(read_system(Path::new("//host/share/gitconfig")).is_none());
}

#[test]
fn read_system_parses_a_small_file() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("gitconfig");
    std::fs::write(&path, "[core]\n\tautocrlf = true\n").expect("write");
    let file = read_system(&path).expect("parsed");
    assert_eq!(
        file.string("core.autocrlf").expect("value").to_string(),
        "true"
    );
}

/// A repository whose own config gets `extra` appended, opened with isolated config.
fn repo_with(extra: &str) -> (Fixture, gix::Repository) {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"a")]);
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.path().join(".git/config"))
        .expect("open config");
    config.write_all(extra.as_bytes()).expect("append");
    let repo = Repo::open_isolated(fixture.path()).expect("open").local();
    (fixture, repo)
}

fn system(text: &str) -> gix::config::File {
    gix::config::File::try_from(text).expect("system config")
}

#[test]
fn the_system_config_fills_in_a_silent_repository() {
    let (_fixture, repo) = repo_with("");
    let sys = system("[core]\n\tautocrlf = true\n");
    let found = eol_settings(&repo, Some(&sys)).expect("settings");
    assert_eq!(found.auto_crlf, AutoCrlf::Enabled);
}

#[test]
fn the_repository_beats_the_system_config() {
    let (_fixture, repo) = repo_with("[core]\n\tautocrlf = false\n");
    let sys = system("[core]\n\tautocrlf = true\n");
    let found = eol_settings(&repo, Some(&sys)).expect("settings");
    assert_eq!(found.auto_crlf, AutoCrlf::Disabled);
}

#[test]
fn autocrlf_input_and_core_eol_are_read() {
    let (_fixture, repo) = repo_with("[core]\n\tautocrlf = input\n\teol = crlf\n");
    let found = eol_settings(&repo, None).expect("settings");
    assert_eq!(found.auto_crlf, AutoCrlf::Input);
    assert_eq!(found.eol, Some(Mode::CrLf));
}

#[test]
fn nothing_set_means_disabled_and_no_eol() {
    let (_fixture, repo) = repo_with("");
    let found = eol_settings(&repo, None).expect("settings");
    assert_eq!(found.auto_crlf, AutoCrlf::Disabled);
    assert_eq!(found.eol, None);
}

#[test]
fn an_invalid_autocrlf_is_broken() {
    let (_fixture, repo) = repo_with("[core]\n\tautocrlf = maybe\n");
    assert_eq!(eol_settings(&repo, None).err(), Some(GitProblem::Broken));
}

#[test]
fn a_network_attributes_file_is_unsupported() {
    for value in ["//host/x", r#"":(optional)\\\\host\\x""#] {
        let (_fixture, repo) = repo_with(&format!("[core]\n\tattributesFile = {value}\n"));
        assert_eq!(
            check_attributes_file(&repo),
            Err(GitProblem::Unsupported),
            "{value}"
        );
    }
}

#[test]
fn a_local_attributes_file_is_accepted() {
    let (_fixture, repo) = repo_with("[core]\n\tattributesFile = /tmp/attrs\n");
    assert_eq!(check_attributes_file(&repo), Ok(()));
}
