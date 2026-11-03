use std::ffi::OsString;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use navo_git::status::GitStatus;
use navo_git::{Cancel, CliGit, GitError};

#[cfg(unix)]
use navo_git::status::Reason;
#[cfg(unix)]
use navo_git::trust::{Guard, untrusted_reason};

use super::cli_status_support::{repo, sibling};

#[cfg(unix)]
use super::fixture::Fixture;
#[cfg(unix)]
use std::path::PathBuf;

const CHILD_FLAG: &str = "NAVO_GS4_CHILD";

#[cfg(unix)]
fn marker(fixture: &Fixture) -> PathBuf {
    sibling(fixture, "marker")
}

#[cfg(unix)]
fn program(fixture: &Fixture) -> PathBuf {
    let script = format!("#!/bin/sh\ntouch '{}'\ncat\n", marker(fixture).display());
    fixture.script("evil.sh", &script)
}

#[cfg(unix)]
fn run_status(fixture: &Fixture) -> navo_git::status::RepoStatus {
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap()
}

/// Same size as the committed file and an old mtime, so only reading the content shows the change.
#[cfg(unix)]
fn edit(fixture: &Fixture, bytes: &[u8], age_secs: u64) {
    fixture.write("a.txt", bytes);
    let old = std::time::SystemTime::now() - Duration::from_secs(age_secs);
    fs::File::options()
        .write(true)
        .open(fixture.repo().join("a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
}

#[cfg(unix)]
fn assert_guarded_status_runs_nothing(fixture: &Fixture) {
    let marker = marker(fixture);
    edit(fixture, b"aaaa\n", 3600);
    fixture.git_ok(&["status", "--porcelain"]);
    assert!(
        marker.exists(),
        "the plain status must run it, or the setup proves nothing"
    );
    fs::remove_file(&marker).unwrap();
    edit(fixture, b"bbbb\n", 7200);
    let status = run_status(fixture);
    assert!(!marker.exists(), "Navo's status ran repository config");
    assert_eq!(status.changed, 1);
}

#[cfg(unix)]
#[test]
fn a_clean_filter_in_the_repository_config_does_not_run() {
    let Some(fixture) = repo() else { return };
    fixture.write(".gitattributes", b"*.txt filter=evil\n");
    fixture.git_ok(&["add", ".gitattributes"]);
    fixture.commit("attributes");
    let script = program(&fixture);
    fixture.git_ok(&["config", "filter.evil.clean", script.to_str().unwrap()]);
    assert_guarded_status_runs_nothing(&fixture);
}

#[cfg(unix)]
#[test]
fn an_fsmonitor_program_in_the_repository_config_does_not_run() {
    let Some(fixture) = repo() else { return };
    let script = program(&fixture);
    fixture.git_ok(&["config", "core.fsmonitor", script.to_str().unwrap()]);
    assert_guarded_status_runs_nothing(&fixture);
}

#[cfg(unix)]
#[test]
fn a_fifo_include_is_not_trusted_and_status_never_starts() {
    let Some(fixture) = repo() else { return };
    let (cli, log) = super::cli_status_support::wrapped(&fixture, "");
    let config = fixture.repo().join(".git/config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str("[include]\n\tpath = a.inc\n");
    fs::write(&config, text).unwrap();
    let made = Command::new("mkfifo")
        .arg(fixture.repo().join(".git/a.inc"))
        .status()
        .unwrap();
    assert!(made.success());
    let guard = Guard::read(&fixture.repo().join(".git")).unwrap();
    assert_not_trusted_without_status(&cli, &guard, &log);
}

#[cfg(unix)]
#[test]
fn a_filter_name_the_guard_cannot_express_is_not_trusted() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["config", "filter.a=b.clean", "cat"]);
    let (cli, log) = super::cli_status_support::wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    assert_not_trusted_without_status(&cli, &guard, &log);
}

#[cfg(unix)]
fn assert_not_trusted_without_status(cli: &CliGit, guard: &Guard, log: &std::path::Path) {
    assert!(guard.is_blocked());
    let error = cli.status(guard, &Cancel::default()).unwrap_err();
    assert!(matches!(error, GitError::NotTrusted { .. }), "{error:?}");
    assert_eq!(untrusted_reason(&error), Some(Reason::NotTrusted));
    let calls = fs::read_to_string(log).unwrap();
    assert!(!calls.contains("status"), "git ran: {calls}");
}

#[cfg(unix)]
#[test]
fn the_child_inherits_the_home_of_the_process_unchanged() {
    let Some(fixture) = repo() else { return };
    let (cli, _) = super::cli_status_support::wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap();
    let seen = fs::read_to_string(sibling(&fixture, "git.env")).unwrap();
    let var = |name| {
        std::env::var_os(name).map_or_else(|| "unset".into(), |v| v.to_string_lossy().into_owned())
    };
    let home = std::env::var_os("HOME")
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    assert_eq!(seen.trim_end(), format!("{home}|{}", var("USERPROFILE")));
}

#[cfg(unix)]
#[test]
fn a_dubious_owner_found_by_status_is_unsafe_directory() {
    let Some(fixture) = repo() else { return };
    let (cli, _) = super::cli_status_support::wrapped(
        &fixture,
        "case \" $* \" in *\" status \"*) export GIT_TEST_ASSUME_DIFFERENT_OWNER=1 ;; esac",
    );
    let guard = cli.guard().unwrap();
    let error = cli.status(&guard, &Cancel::default()).unwrap_err();
    assert!(
        matches!(error, GitError::UnsafeDirectory { .. }),
        "{error:?}"
    );
    assert_eq!(untrusted_reason(&error), Some(Reason::NotTrusted));
}

#[cfg(windows)]
#[test]
fn a_dubious_owner_is_unsafe_directory_on_windows() {
    let Some(fixture) = repo() else { return };
    let global = sibling(&fixture, "global-config");
    fs::write(&global, "").unwrap();
    let options = navo_git::CliOptions {
        env: vec![
            (
                OsString::from("GIT_TEST_ASSUME_DIFFERENT_OWNER"),
                OsString::from("1"),
            ),
            (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
            (OsString::from("GIT_CONFIG_GLOBAL"), global.into_os_string()),
        ],
        ..navo_git::CliOptions::default()
    };
    let error = CliGit::with_options(fixture.repo(), &options)
        .and_then(|cli| {
            cli.guard()
                .and_then(|guard| cli.status(&guard, &Cancel::default()))
        })
        .unwrap_err();
    assert!(
        matches!(error, GitError::UnsafeDirectory { .. }),
        "{error:?}"
    );
}

struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        self.0.kill().unwrap_or_default();
        self.0.wait().map(drop).unwrap_or_default();
    }
}

#[test]
fn a_tilde_include_is_seen_the_same_by_the_guard_and_by_git() {
    let Some(fixture) = repo() else { return };
    let home = sibling(&fixture, "home");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("extra.inc"), "[filter \"evil\"]\n\tclean = cat\n").unwrap();
    let report = sibling(&fixture, "child.txt");
    let mut child = Reaped(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli_status_safety::tilde_include_child",
                "--nocapture",
            ])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env(CHILD_FLAG, "1")
            .stdin(Stdio::null())
            .stdout(fs::File::create(&report).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "child hung");
        thread::sleep(Duration::from_millis(5));
    };
    let output = fs::read_to_string(&report).unwrap();
    assert!(status.success() && output.contains("1 passed"), "{output}");
}

#[test]
fn tilde_include_child() {
    if std::env::var_os(CHILD_FLAG).is_none() {
        return;
    }
    let Some(fixture) = repo() else { return };
    let config = fixture.repo().join(".git/config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str("[include]\n\tpath = ~/extra.inc\n");
    fs::write(&config, text).unwrap();
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    assert!(!guard.is_blocked());
    let neutral = guard
        .args()
        .iter()
        .any(|arg| arg == &OsString::from("filter.evil.clean="));
    assert!(
        neutral,
        "the guard missed the ~/ include: {:?}",
        guard.args()
    );
    let seen = fixture.git_ok(&["config", "--get", "filter.evil.clean"]);
    assert_eq!(String::from_utf8_lossy(&seen.stdout).trim(), "cat");
}
