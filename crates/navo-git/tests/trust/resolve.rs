use std::fs;
use std::path::Path;

use navo_git::trust::Guard;
use navo_git::{CliGit, CliOptions};

use super::trust_git::{append_config, fixture, status};
use super::trust_support::neutralised;

#[cfg(unix)]
use super::trust_support::GitDir;
#[cfg(target_os = "linux")]
use super::trust_support::pairs;
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

const HIDDEN: &str = "[filter \"evil\"]\n\tclean = touch filter-ran\n";

fn cli_guard(repo: &Path) -> Guard {
    CliGit::new(repo).unwrap().guard().unwrap()
}

fn assert_cwd_is_not(repo: &Path) {
    let cwd = std::env::current_dir().unwrap();
    assert!(!cwd.starts_with(repo) && !repo.starts_with(&cwd), "{cwd:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_cwd_dependent_include_never_runs_its_filter_from_another_cwd() {
    const MAGIC: &str = "/proc/self/cwd/.git/hidden.inc";
    for symlinked in [false, true] {
        let Some(fixture) = fixture() else {
            return;
        };
        let git_dir = fixture.repo().join(".git");
        append_config(&fixture, "hidden.inc", HIDDEN);
        let include = if symlinked {
            std::os::unix::fs::symlink(MAGIC, git_dir.join("link.inc")).unwrap();
            "link.inc"
        } else {
            MAGIC
        };
        append_config(
            &fixture,
            "config",
            &format!("[include]\n\tpath = {include}\n"),
        );
        assert_cwd_is_not(fixture.repo());
        assert!(status(&fixture, fixture.repo(), &[]).1, "git skipped it");
        assert!(Guard::read(&git_dir).unwrap().is_blocked(), "{symlinked}");
        let guard = cli_guard(fixture.repo());
        assert!(guard.is_blocked(), "symlinked: {symlinked}");
        assert!(neutralised(&guard, "evil"), "{:?}", pairs(&guard));
        assert!(!status(&fixture, fixture.repo(), &guard.args()).1);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn every_process_dependent_include_blocks_the_reader() {
    let repo = GitDir::new("");
    let real = repo.write("real.inc", HIDDEN.as_bytes());
    let plain = GitDir::new(&format!("[include]\n\tpath = {}\n", real.display()));
    assert!(!plain.guard().is_blocked() && neutralised(&plain.guard(), "evil"));
    let links = [
        format!("/proc/self/root{}", real.display()),
        "/proc/self/cwd/real.inc".to_owned(),
        "/proc/self/fd/0".to_owned(),
        "/dev/fd/0".to_owned(),
        "/dev/stdin".to_owned(),
    ];
    for link in links {
        let config = GitDir::new(&format!("[include]\n\tpath = {link}\n"));
        assert!(config.guard().is_blocked(), "{link}");
    }
}

#[cfg(unix)]
#[test]
fn a_fifo_include_blocks_the_reader_without_hanging() {
    let repo = GitDir::new("[include]\n\tpath = a.inc\n");
    let made = std::process::Command::new("mkfifo")
        .arg(repo.git_dir.join("a.inc"))
        .status()
        .unwrap();
    assert!(made.success());
    let git_dir = repo.git_dir.clone();
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        send.send(Guard::read(&git_dir).unwrap())
            .unwrap_or_default()
    });
    let guard = receive
        .recv_timeout(Duration::from_secs(10))
        .expect("the reader hung on a FIFO");
    assert!(guard.is_blocked());
}

#[test]
fn a_relative_include_resolves_beside_its_config_whatever_the_cwd() {
    let Some(fixture) = fixture() else {
        return;
    };
    fs::create_dir_all(fixture.repo().join(".git/sub")).unwrap();
    append_config(&fixture, "sub/hidden.inc", HIDDEN);
    append_config(&fixture, "config", "[include]\n\tpath = sub/hidden.inc\n");
    assert_cwd_is_not(fixture.repo());
    let guard = cli_guard(fixture.repo());
    assert!(!guard.is_blocked() && neutralised(&guard, "evil"));
    assert!(!status(&fixture, fixture.repo(), &guard.args()).1);
    assert!(status(&fixture, fixture.repo(), &[]).1, "git skipped it");
}

#[cfg(unix)]
#[test]
fn a_symlinked_include_to_a_plain_file_is_read_and_neutralised() {
    let Some(fixture) = fixture() else {
        return;
    };
    let target = fixture.repo().with_file_name("elsewhere.inc");
    fs::write(&target, HIDDEN).unwrap();
    let link = fixture.repo().join(".git").join("link.inc");
    std::os::unix::fs::symlink(&target, link).unwrap();
    append_config(&fixture, "config", "[include]\n\tpath = link.inc\n");
    let guard = cli_guard(fixture.repo());
    assert!(!guard.is_blocked() && neutralised(&guard, "evil"));
    assert!(!status(&fixture, fixture.repo(), &guard.args()).1);
    assert!(status(&fixture, fixture.repo(), &[]).1, "git skipped it");
}

#[test]
fn a_rewritten_home_blocks_the_guard() {
    let Some(fixture) = fixture() else {
        return;
    };
    assert!(!cli_guard(fixture.repo()).is_blocked());
    for key in ["HOME", "USERPROFILE", "home"] {
        let options = CliOptions {
            env: vec![(key.into(), fixture.repo().as_os_str().to_owned())],
            ..CliOptions::default()
        };
        let cli = CliGit::with_options(fixture.repo(), &options).unwrap();
        assert!(cli.guard().unwrap().is_blocked(), "{key}");
    }
}
