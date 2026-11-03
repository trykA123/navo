use std::fs;

use navo_git::{CliGit, GitError};

use super::cli_remote_support::{repo, sibling, with_origin};
use super::cli_sync_support::{change_on_origin, cli, isolated, old_pull, pull, snapshot};
use super::fixture::Fixture;

fn theirs_on_origin() -> Option<Fixture> {
    let fixture = repo()?;
    let origin = with_origin(&fixture);
    change_on_origin(&fixture, &origin, "clone2", ("a.txt", b"theirs\n"));
    Some(fixture)
}

fn head(fixture: &Fixture) -> Vec<u8> {
    fixture.git_ok(&["rev-parse", "HEAD"]).stdout
}

#[test]
fn merge_autostash_cannot_turn_a_blocked_pull_into_conflict_markers() {
    let Some(control) = theirs_on_origin() else {
        return;
    };
    control.git_ok(&["config", "merge.autoStash", "true"]);
    control.write("a.txt", b"mine\n");
    let output = old_pull(&cli(&control));
    let text = fs::read_to_string(control.repo().join("a.txt")).unwrap();
    assert!(
        output.status.unwrap().success() && text.contains("<<<<<<<"),
        "the control no longer shows the autostash conflict: {text}"
    );

    let fixture = theirs_on_origin().unwrap();
    fixture.git_ok(&["config", "merge.autoStash", "true"]);
    fixture.write("a.txt", b"mine\n");
    let before = snapshot(&fixture);
    let error = pull(&cli(&fixture)).unwrap_err();
    assert!(matches!(error, GitError::LocalChanges), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
    assert!(fixture.git_ok(&["stash", "list"]).stdout.is_empty());
}

#[test]
fn branch_merge_options_squash_cannot_fake_an_up_to_date_pull() {
    let Some(control) = theirs_on_origin() else {
        return;
    };
    control.git_ok(&["config", "branch.main.mergeOptions", "--squash"]);
    let before = head(&control);
    let output = old_pull(&cli(&control));
    assert!(output.status.unwrap().success());
    assert_eq!(head(&control), before, "the control squash moved HEAD");

    let fixture = theirs_on_origin().unwrap();
    fixture.git_ok(&["config", "branch.main.mergeOptions", "--squash"]);
    let summary = pull(&cli(&fixture)).unwrap();
    assert_eq!(summary.commits, 1);
    assert_eq!(
        head(&fixture),
        fixture.git_ok(&["rev-parse", "origin/main"]).stdout
    );
}

#[cfg(unix)]
#[test]
fn an_exit_zero_pull_that_did_not_reach_the_upstream_is_an_error() {
    use super::cli_sync_support::script_hook;

    let Some(fixture) = theirs_on_origin() else {
        return;
    };
    let hooks = script_hook(&fixture, "post-merge", "git reset -q --soft HEAD~1");
    fixture.git_ok(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    let error = pull(&cli(&fixture)).unwrap_err();
    assert!(matches!(error, GitError::PullUnfinished), "{error:?}");
}

fn tag_and_drop_branch_on_origin() -> Option<Fixture> {
    let fixture = repo()?;
    let origin = with_origin(&fixture);
    fixture.git_ok(&["push", "-q", "origin", "main:refs/heads/tmpb"]);
    fixture.git_ok(&["fetch", "-q", "origin"]);
    let clone = sibling(&fixture, "clone2");
    fixture.git_ok(&["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    fs::write(clone.join("b.txt"), b"b\n").unwrap();
    let clone = clone.to_str().unwrap();
    fixture.git_ok(&["-C", clone, "add", "."]);
    fixture.git_ok(&["-C", clone, "commit", "-m", "b"]);
    fixture.git_ok(&["-C", clone, "tag", "v9"]);
    fixture.git_ok(&["-C", clone, "push", "origin", "main", "v9", ":tmpb"]);
    fixture.git_ok(&["config", "fetch.prune", "true"]);
    fixture.git_ok(&["config", "fetch.pruneTags", "true"]);
    fixture.git_ok(&["config", "remote.origin.pruneTags", "true"]);
    Some(fixture)
}

fn kept(fixture: &Fixture) -> (bool, bool) {
    let tmpb = fixture.git(&["rev-parse", "--verify", "-q", "refs/remotes/origin/tmpb"]);
    let tag = fixture.git(&["rev-parse", "--verify", "-q", "refs/tags/v9"]);
    (tmpb.status.success(), tag.status.success())
}

#[test]
fn pull_never_prunes_remote_tracking_refs_and_creates_no_tag() {
    let Some(control) = tag_and_drop_branch_on_origin() else {
        return;
    };
    assert!(old_pull(&cli(&control)).status.unwrap().success());
    assert_eq!(
        kept(&control),
        (false, true),
        "the control no longer prunes"
    );

    let fixture = tag_and_drop_branch_on_origin().unwrap();
    assert_eq!(pull(&cli(&fixture)).unwrap().commits, 1);
    assert_eq!(kept(&fixture), (true, false));
}

fn traced(fixture: &Fixture) -> (CliGit, std::path::PathBuf) {
    let trace = sibling(fixture, "trace.log");
    let mut options = isolated(fixture);
    options.env.push(("GIT_TRACE".into(), trace.clone().into()));
    (
        CliGit::with_options(fixture.repo(), &options).unwrap(),
        trace,
    )
}

#[test]
fn pull_starts_no_maintenance() {
    let Some(control) = theirs_on_origin() else {
        return;
    };
    let (cli, trace) = traced(&control);
    assert!(old_pull(&cli).status.unwrap().success());
    let text = fs::read_to_string(&trace).unwrap();
    assert!(text.contains("maintenance run"), "control: {text}");

    let fixture = theirs_on_origin().unwrap();
    let (cli, trace) = traced(&fixture);
    assert_eq!(pull(&cli).unwrap().commits, 1);
    let text = fs::read_to_string(&trace).unwrap();
    assert!(text.contains(" merge "), "{text}");
    assert!(!text.contains("maintenance run"), "{text}");
    assert!(!text.contains(" gc "), "{text}");
}

fn local_refs(fixture: &Fixture) -> Vec<u8> {
    fixture.git_ok(&["for-each-ref"]).stdout
}

#[test]
fn fetch_all_cannot_make_a_pull_fetch_other_remotes() {
    let Some(fixture) = theirs_on_origin() else {
        return;
    };
    let other = sibling(&fixture, "o2.git");
    fixture.git_ok(&["init", "--bare", "-b", "main", other.to_str().unwrap()]);
    fixture.git_ok(&[
        "push",
        "-q",
        other.to_str().unwrap(),
        "main:refs/heads/side",
    ]);
    fixture.git_ok(&["remote", "add", "o2", other.to_str().unwrap()]);
    fixture.git_ok(&["config", "fetch.all", "true"]);
    assert_eq!(pull(&cli(&fixture)).unwrap().commits, 1);
    let refs = String::from_utf8(local_refs(&fixture)).unwrap();
    assert!(!refs.contains("refs/remotes/o2/"), "{refs}");
}

#[test]
fn a_fetch_refspec_beyond_the_default_is_refused_before_any_pull() {
    let tags = ["--add", "remote.origin.fetch", "+refs/tags/*:refs/tags/*"];
    let heads = [
        "--replace-all",
        "remote.origin.fetch",
        "+refs/heads/*:refs/heads/*",
    ];
    for change in [&tags[..], &heads[..]] {
        let fixture = theirs_on_origin().unwrap();
        let mut args = vec!["config"];
        args.extend_from_slice(change);
        fixture.git_ok(&args);
        let (before, refs) = (snapshot(&fixture), local_refs(&fixture));
        let error = pull(&cli(&fixture)).unwrap_err();
        assert!(
            matches!(error, GitError::UnsupportedRefspec),
            "{change:?}: {error:?}"
        );
        assert_eq!(snapshot(&fixture), before);
        assert_eq!(local_refs(&fixture), refs);
    }
}

#[test]
fn the_default_fetch_refspec_still_pulls() {
    let fixture = theirs_on_origin().unwrap();
    let default = fixture.git_ok(&["config", "--get-all", "remote.origin.fetch"]);
    assert_eq!(default.stdout, b"+refs/heads/*:refs/remotes/origin/*\n");
    assert_eq!(pull(&cli(&fixture)).unwrap().commits, 1);
}
