use super::*;

#[test]
fn pull_and_push_confirmations_name_branch_remote_count_and_guarded_keys() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let ask = wired.preview_text(GitAction::Pull).unwrap();
    for word in ["main", "origin", "2", "core.hookspath"] {
        assert!(ask.contains(word), "{word} in {ask}");
    }
    assert!(wired.preview_text(GitAction::Push).is_some());
    assert!(wired.preview_text(GitAction::Commit).is_none());
}

#[test]
fn a_tool_that_opens_its_own_dialog_asks_nothing() {
    let wired = wired("tortoisegit", &[ToolKind::TortoiseGit]);
    assert!(!wired.item(GitAction::Pull).pending);
    assert!(wired.previews().is_empty());
    assert!(wired.sync.confirms.lock().unwrap().is_empty());
}

#[test]
fn a_menu_that_is_never_confirmed_runs_nothing() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    wired.actions();
    assert!(wired.sync.confirms.lock().unwrap().is_empty());
    assert!(wired.sync.runs.lock().unwrap().is_empty());
    assert!(wired.tools.launches.lock().unwrap().is_empty());
    wired.settled_rescans(0);
}

#[test]
fn pull_without_a_shown_confirmation_is_refused_before_any_process() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    assert!(wired.rig.hooks.run(&wired.repo, GitAction::Pull).is_err());
    assert!(wired.sync.runs.lock().unwrap().is_empty());
    wired.settled_rescans(0);
}

#[test]
fn a_confirmed_pull_runs_the_shown_confirmation_once_and_rescans_once() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let shown = wired.preview(GitAction::Pull).unwrap();
    let started = wired.run_shown(GitAction::Pull, Some(&shown)).unwrap();
    let result = started.result.unwrap();
    assert!(
        result.contains("main") && result.contains("origin"),
        "{result}"
    );
    {
        let runs = wired.sync.runs.lock().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].1, confirmation(SyncKind::Pull));
    }
    assert!(shown.text.contains("main"));
    assert!(wired.run_shown(GitAction::Pull, Some(&shown)).is_err());
    assert_eq!(wired.sync.runs.lock().unwrap().len(), 1);
    wired.settled_rescans(1);
}

#[test]
fn a_pull_whose_tracking_ref_lags_still_reports_done_and_rescans_once() {
    let stale = SyncSummary {
        tracking_stale: true,
        ..pulled()
    };
    let wired = wired_with("git-cli", &[ToolKind::GitCli], Ok(stale));
    let shown = wired.preview(GitAction::Pull);
    let started = wired.run_shown(GitAction::Pull, shown.as_ref()).unwrap();
    assert_eq!(
        started.result,
        Some(super::super::super::texts::done_text(&pulled()))
    );
    wired.settled_rescans(1);
}

#[test]
fn a_confirmation_shown_for_another_head_runs_nothing() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let first = wired.preview(GitAction::Pull).unwrap();
    *wired.sync.head.lock().unwrap() = Some("2".repeat(40));
    let second = wired.preview(GitAction::Pull).unwrap();
    assert_ne!(first.text, second.text);
    assert!(wired.run_shown(GitAction::Pull, Some(&first)).is_err());
    assert!(wired.rig.hooks.run(&wired.repo, GitAction::Pull).is_err());
    assert!(wired.sync.runs.lock().unwrap().is_empty());
    wired.run_shown(GitAction::Pull, Some(&second)).unwrap();
    let runs = wired.sync.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1.head, Some("2".repeat(40)));
    wired.settled_rescans(1);
}

#[test]
fn an_older_preview_of_the_same_text_never_runs_the_newer_target() {
    let differ_in = [
        (
            "url",
            ("/a/origin.git", "/a/hooks"),
            ("/b/origin.git", "/a/hooks"),
        ),
        (
            "guard",
            ("/a/origin.git", "/a/hooks"),
            ("/a/origin.git", "/b/hooks"),
        ),
    ];
    for (what, (url_a, hooks_a), (url_b, hooks_b)) in differ_in {
        let wired = wired("git-cli", &[ToolKind::GitCli]);
        let target_a = confirmation_of(SyncKind::Pull, url_a, hooks_a);
        let target_b = confirmation_of(SyncKind::Pull, url_b, hooks_b);
        assert_ne!(target_a, target_b, "{what}");
        *wired.sync.queued.lock().unwrap() = vec![target_a, target_b.clone()];
        let window_a = wired.preview(GitAction::Pull).unwrap();
        let window_b = wired.preview(GitAction::Pull).unwrap();
        assert_eq!(window_a.text, window_b.text, "{what}");
        let refused = wired.run_shown(GitAction::Pull, Some(&window_a));
        assert_eq!(
            refused.unwrap_err(),
            navo_l10n::tr("git-fail-reopen"),
            "{what}"
        );
        assert!(wired.sync.runs.lock().unwrap().is_empty(), "{what}");
        wired.run_shown(GitAction::Pull, Some(&window_b)).unwrap();
        let runs = wired.sync.runs.lock().unwrap().clone();
        assert_eq!(runs.len(), 1, "{what}");
        assert_eq!(runs[0].1, target_b, "{what}");
        wired.settled_rescans(1);
    }
}

#[test]
fn a_run_without_the_id_or_with_another_actions_id_runs_nothing() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let replies = wired.previews();
    let shown = |action| replies.iter().find(|(held, _)| *held == action);
    let pull = shown(GitAction::Pull).unwrap().1.clone().unwrap();
    let push = shown(GitAction::Push).unwrap().1.clone().unwrap();
    let hooks = &wired.rig.hooks;
    let text = Some(pull.text.as_str());
    assert!(
        hooks
            .run_confirmed(&wired.repo, GitAction::Pull, text, None)
            .is_err()
    );
    let foreign = Some(push.id.as_str());
    assert!(
        hooks
            .run_confirmed(&wired.repo, GitAction::Pull, text, foreign)
            .is_err()
    );
    let unknown = Some("no such id");
    assert!(
        hooks
            .run_confirmed(&wired.repo, GitAction::Pull, text, unknown)
            .is_err()
    );
    assert!(hooks.run(&wired.repo, GitAction::Pull).is_err());
    assert!(hooks.run(&wired.repo, GitAction::Push).is_err());
    assert!(wired.sync.runs.lock().unwrap().is_empty());
    wired.run_shown(GitAction::Pull, Some(&pull)).unwrap();
    wired.settled_rescans(1);
}

#[test]
fn a_launched_tool_gets_its_plan_and_one_rescan_when_it_exits() {
    let wired = wired("tortoisegit", &[ToolKind::TortoiseGit]);
    let started = wired.rig.hooks.run(&wired.repo, GitAction::Commit).unwrap();
    assert_eq!(started.tool, "TortoiseGit");
    assert_eq!(started.result, None);
    let launches = wired.tools.launches.lock().unwrap();
    assert!(matches!(launches.as_slice(), [Launch::Process(_)]));
    drop(launches);
    wired.settled_rescans(1);
}

#[test]
fn git_cli_commit_and_log_open_the_terminal_in_the_repository() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    wired.rig.hooks.run(&wired.repo, GitAction::Commit).unwrap();
    wired.rig.hooks.run(&wired.repo, GitAction::Log).unwrap();
    let launches = wired.tools.launches.lock().unwrap();
    assert_eq!(launches.len(), 2);
    assert!(
        launches
            .iter()
            .all(|launch| matches!(launch, Launch::Terminal { .. }))
    );
}

#[test]
fn run_without_a_tool_explains_instead_of_starting() {
    let wired = wired("none", &[]);
    assert!(wired.rig.hooks.run(&wired.repo, GitAction::Commit).is_err());
    assert!(wired.tools.launches.lock().unwrap().is_empty());
}

#[test]
fn interrupted_and_unfinished_pulls_always_warn_about_the_working_tree() {
    let warning = navo_l10n::tr("git-tree-may-have-changed");
    let failures = [
        SyncFailure::Interrupted {
            lock: false,
            moved: false,
        },
        SyncFailure::Interrupted {
            lock: true,
            moved: true,
        },
        SyncFailure::Unfinished,
    ];
    for failure in failures {
        let wired = wired_with("git-cli", &[ToolKind::GitCli], Err(failure));
        let shown = wired.preview(GitAction::Pull);
        let error = wired
            .run_shown(GitAction::Pull, shown.as_ref())
            .unwrap_err();
        assert!(error.contains(&warning), "{failure:?}: {error}");
        wired.settled_rescans(1);
    }
}

#[test]
fn every_refusal_gets_its_own_status_text() {
    let failures = [
        SyncFailure::NotFastForward,
        SyncFailure::LocalChanges,
        SyncFailure::Rejected,
        SyncFailure::AuthFailed,
        SyncFailure::NoUpstream,
        SyncFailure::UnsafeRemote,
        SyncFailure::UnsupportedRefspec,
        SyncFailure::RemoteBranchGone,
        SyncFailure::Detached,
        SyncFailure::ConfigChanged,
        SyncFailure::TargetChanged,
        SyncFailure::Cancelled,
        SyncFailure::TimedOut,
        SyncFailure::NoGit,
        SyncFailure::NotTrusted,
        SyncFailure::Failed,
    ];
    let mut seen = std::collections::HashSet::new();
    for failure in failures {
        let wired = wired_with("git-cli", &[ToolKind::GitCli], Err(failure));
        let shown = wired.preview(GitAction::Push);
        let error = wired
            .run_shown(GitAction::Push, shown.as_ref())
            .unwrap_err();
        assert!(!error.is_empty());
        assert!(seen.insert(error), "{failure:?} repeats another text");
    }
}
