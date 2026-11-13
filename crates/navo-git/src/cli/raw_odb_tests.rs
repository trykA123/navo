//! Unit tests for raw-ODB reads and promisor-remote classification in `CliGit`.

use std::{ffi::OsString, fs, io::Read, path::Path, process::Command};

use super::{is_missing_promisor_stderr as is_promisor, parse_has_promisor};
use crate::{CliGit, CliOptions, GitError, GitPath, GitSource};

fn isolated_env() -> Vec<(OsString, OsString)> {
    vec![
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
    ]
}

fn g_output(cwd: &Path, args: &[&str]) -> std::process::Output {
    let bin = if cfg!(windows) { "git.exe" } else { "git" };
    let out = Command::new(bin)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    out
}

fn g(cwd: &Path, args: &[&str]) {
    g_output(cwd, args);
}

fn commit_one(root: &Path, name: &str, file: &str, body: &[u8]) {
    g(root, &["init", "-q", name]);
    let work = root.join(name);
    g(&work, &["config", "user.name", "N"]);
    g(&work, &["config", "user.email", "n@x"]);
    fs::write(work.join(file), body).unwrap();
    g(&work, &["add", "-A"]);
    g(&work, &["commit", "-qm", "1"]);
}

fn raw_read(
    repo: &Path,
    mut env: Vec<(OsString, OsString)>,
    path: &str,
) -> Result<Vec<u8>, GitError> {
    let mut isolated = isolated_env();
    isolated.append(&mut env);
    let opts = CliOptions {
        env: isolated,
        raw_odb_only: true,
        ..Default::default()
    };
    let mut blob =
        CliGit::with_options(repo, &opts)?.read_blob_worktree_form("HEAD", &GitPath::from(path))?;
    assert!(blob.reason.is_none());
    let mut out = Vec::new();
    blob.reader.read_to_end(&mut out).unwrap();
    Ok(out)
}

#[test]
fn raw_odb_skips_hostile_filters_in_bare_repo() {
    let t = tempfile::tempdir().unwrap();
    let (root, bare) = (t.path(), t.path().join("b.git"));
    commit_one(root, "w", "data.txt", b"raw-odb-bytes\n");
    g(root, &["clone", "-q", "--bare", "w", "b.git"]);
    let attrs = bare.join("info/attributes");
    fs::write(&attrs, "* filter=hostile\n").unwrap();
    let pairs = [
        ("core.attributesfile", attrs.to_str().unwrap()),
        ("filter.hostile.smudge", "nonexistent-hostile-filter"),
        ("filter.hostile.required", "true"),
    ];
    let mut env = vec![("GIT_CONFIG_COUNT".into(), "3".into())];
    for (i, (k, v)) in pairs.iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{i}").into(), (*k).into()));
        env.push((format!("GIT_CONFIG_VALUE_{i}").into(), (*v).into()));
    }
    let got = raw_read(&bare, env, "data.txt").unwrap();
    assert_eq!(got, b"raw-odb-bytes\n");
}

#[test]
fn raw_odb_reports_missing_promisor_blob_with_no_lazy_fetch() {
    let t = tempfile::tempdir().unwrap();
    let (root, remote) = (t.path(), t.path().join("r.git"));
    g(root, &["init", "-q", "--bare", "r.git"]);
    commit_one(root, "w", "blob.txt", b"lazy-content\n");
    let w = root.join("w");
    g(&w, &["push", "-q", "../r.git", "HEAD:refs/heads/main"]);
    g(&remote, &["config", "uploadpack.allowFilter", "true"]);
    g(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let url = format!("file://{}", remote.display());
    let args = ["clone", "-q", "--bare", "--filter=blob:none", &url, "p.git"];
    g(root, &args);
    let err = raw_read(
        &root.join("p.git"),
        vec![("GIT_NO_LAZY_FETCH".into(), "1".into())],
        "blob.txt",
    );
    assert!(matches!(err, Err(GitError::MissingPromisorBlob)));
    assert!(is_promisor("from promisor remote", true, false));
    assert!(is_promisor("lazy fetching disabled", false, false));
    assert!(!is_promisor(
        "fatal: git cat-file HEAD:blob.txt: bad file",
        true,
        false
    ));
    assert!(is_promisor(
        "fatal: git cat-file HEAD:blob.txt: bad file",
        true,
        true
    ));
    assert!(!is_promisor(
        "fatal: git cat-file HEAD:blob.txt: bad file",
        false,
        true
    ));
}

#[test]
fn parse_has_promisor_checks_promisor_remote_and_partial_clone() {
    assert!(!parse_has_promisor(b"core.bare\nfalse\0"));
    assert!(!parse_has_promisor(b"remote.origin.promisor\nfalse\0"));
    assert!(!parse_has_promisor(b"extensions.partialclone\n\0"));
    assert!(parse_has_promisor(b"remote.origin.promisor\ntrue\0"));
    assert!(parse_has_promisor(b"remote.origin.promisor\0"));
    assert!(parse_has_promisor(
        b"remote.my.remote.promisor\n1\0remote.origin.promisor\nfalse\0"
    ));
    assert!(!parse_has_promisor(
        b"remote.origin.promisor\ntrue\0remote.origin.promisor\nfalse\0"
    ));
    assert!(parse_has_promisor(b"extensions.partialclone\norigin\0"));
}

fn delete_loose_object(work: &Path, spec: &str) {
    let oid = String::from_utf8(g_output(work, &["rev-parse", spec]).stdout).unwrap();
    let oid = oid.trim();
    let loose = work.join(".git/objects").join(&oid[..2]).join(&oid[2..]);
    #[cfg(windows)]
    {
        let mut perms = fs::metadata(&loose).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(&loose, perms).unwrap();
    }
    fs::remove_file(&loose).unwrap();
}

#[test]
fn raw_odb_reports_plain_read_error_when_loose_object_is_deleted_in_normal_repo() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    commit_one(root, "w", "blob.txt", b"normal-content\n");
    let work = root.join("w");
    delete_loose_object(&work, "HEAD:blob.txt");

    let err = raw_read(&work, Vec::new(), "blob.txt");
    assert!(
        matches!(err, Err(GitError::Command { .. })),
        "expected plain GitError::Command, got {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn raw_odb_caches_promisor_remote_check_per_repo_handle() {
    use std::os::unix::fs::PermissionsExt;

    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    commit_one(root, "w", "a.txt", b"first\n");
    let work = root.join("w");
    fs::write(work.join("b.txt"), b"second\n").unwrap();
    g(&work, &["add", "-A"]);
    g(&work, &["commit", "-qm", "2"]);
    delete_loose_object(&work, "HEAD:a.txt");
    delete_loose_object(&work, "HEAD:b.txt");

    let log = root.join("git-calls.log");
    let wrapper = root.join("git-wrap");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec git \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let opts = CliOptions {
        git: Some(wrapper),
        env: isolated_env(),
        raw_odb_only: true,
        ..Default::default()
    };
    let cli = CliGit::with_options(&work, &opts).unwrap();
    let first = cli.read_blob_worktree_form("HEAD", &GitPath::from("a.txt"));
    let second = cli.read_blob_worktree_form("HEAD", &GitPath::from("b.txt"));
    assert!(matches!(first, Err(GitError::Command { .. })));
    assert!(matches!(second, Err(GitError::Command { .. })));

    let calls = fs::read_to_string(&log).unwrap();
    let config_calls = calls
        .lines()
        .filter(|line| line.contains("config --list"))
        .count();
    assert_eq!(config_calls, 1, "{calls}");
}
