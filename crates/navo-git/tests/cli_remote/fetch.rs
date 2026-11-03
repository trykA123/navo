use std::fs;

use navo_git::CliGit;
use navo_git::status::Head;

use super::cli_remote_support::{fetch, push_from_clone, repo, sibling, status, with_origin};

#[test]
fn fetch_moves_behind_from_zero_to_one_and_writes_fetch_head() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let cli = CliGit::new(fixture.repo()).unwrap();
    let before = status(&cli);
    assert_eq!(before.upstream.as_ref().map(|u| u.behind), Some(0));
    let fetch_head = fixture.repo().join(".git/FETCH_HEAD");
    assert!(!fetch_head.exists());
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    let after = status(&cli);
    assert_eq!(
        after.upstream.as_ref().map(|u| (u.ahead, u.behind)),
        Some((0, 1))
    );
    assert!(fs::metadata(&fetch_head).unwrap().len() > 0);
    assert!(matches!(after.head, Head::Branch(_)));
    assert!(after.fetched_ms.is_some());
}

#[test]
fn fetch_never_touches_the_worktree_the_branch_or_the_tags() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    fixture.git_ok(&[
        "-C",
        sibling(&fixture, "clone2").to_str().unwrap(),
        "tag",
        "v1",
    ]);
    fixture.git_ok(&[
        "-C",
        sibling(&fixture, "clone2").to_str().unwrap(),
        "push",
        "origin",
        "v1",
    ]);
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    let cli = CliGit::new(fixture.repo()).unwrap();
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    assert_eq!(fixture.git_ok(&["rev-parse", "HEAD"]).stdout, head);
    assert_eq!(fs::read(fixture.repo().join("a.txt")).unwrap(), b"base\n");
    assert!(!fixture.repo().join("clone2.txt").exists());
}

#[cfg(unix)]
#[test]
fn fetch_runs_the_command_of_d100g_and_nothing_more() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, log) = super::cli_remote_support::wrapped(&fixture, "");
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    let calls = fs::read_to_string(log).unwrap();
    let line = calls.lines().find(|l| l.contains(" fetch ")).unwrap();
    assert!(
        line.ends_with(
            "-c credential.interactive=never fetch --quiet --no-recurse-submodules --no-prune --no-auto-maintenance origin"
        ),
        "{line}"
    );
    for word in ["--prune", "--tags", "--force", "-f ", "--all"] {
        assert!(!line.contains(word), "{line}");
    }
    assert!(line.contains("-c gc.auto=0") && line.contains("-c core.hooksPath="));
}

#[cfg(unix)]
#[test]
fn the_fetch_child_has_no_prompt_and_inherits_the_home_unchanged() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, _) = super::cli_remote_support::wrapped(&fixture, "");
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    let seen = fs::read_to_string(sibling(&fixture, "git.env")).unwrap();
    let var = |name| {
        std::env::var_os(name).map_or_else(|| "unset".into(), |v| v.to_string_lossy().into_owned())
    };
    let home = std::env::var_os("HOME")
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        seen.trim_end(),
        format!("{home}|{}|never|never|0|", var("USERPROFILE"))
    );
}
