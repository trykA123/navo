use super::menu::{Choice, Model, Source, Via, build};
use super::{BadgeKind, GitAction, GitMenu, GitMenuItem, Previewed, RepoBadge, Shown};

fn shown(text: &str, id: &str) -> Option<Shown> {
    Some(Shown {
        text: text.to_owned(),
        id: id.to_owned(),
    })
}

fn item(action: GitAction, label: &str, confirm: Option<&str>) -> GitMenuItem {
    GitMenuItem {
        action,
        label: label.to_owned(),
        confirm: confirm.map(str::to_owned),
        confirm_id: None,
        pending: false,
    }
}

fn full(confirm: bool) -> GitMenu {
    let ask = |text| confirm.then_some(text);
    GitMenu {
        head: "navo".to_owned(),
        items: vec![
            item(GitAction::Commit, "Commit…", None),
            item(GitAction::Pull, "Pull", ask("Pull main from origin")),
            item(GitAction::Push, "Push", ask("Push main to origin")),
            item(GitAction::Log, "Log", None),
            item(GitAction::Diff, "Diff", None),
        ],
    }
}

fn badge(kind: BadgeKind) -> RepoBadge {
    RepoBadge {
        kind,
        is_root: true,
        stale: false,
        branch: Some("main".to_owned()),
        upstream: Some("origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    }
}

fn model(menu: &GitMenu, badge: &RepoBadge) -> Model {
    let tool = Some("TortoiseGit");
    build(&Source {
        menu,
        badge,
        tool,
        can_diff: true,
        previews: None,
    })
}

fn order(model: &Model) -> Vec<Choice> {
    model.rows.iter().map(|row| row.choice).collect()
}

fn run(action: GitAction) -> Choice {
    Choice::Run(action)
}

fn row(model: &Model, action: GitAction) -> &super::menu::Row {
    let found = model.rows.iter().find(|row| row.choice == run(action));
    found.expect("row")
}

#[test]
fn a_repository_with_changes_leads_with_commit() {
    let changed = RepoBadge {
        changed: 3,
        untracked: 1,
        ..badge(BadgeKind::Modified)
    };
    let shown = model(&full(false), &changed);
    assert_eq!(shown.rows[0].choice, run(GitAction::Commit));
    assert!(shown.rows[0].lead);
    assert_eq!(shown.rows[0].note, "4 changed · 1 new");
    assert_eq!(shown.rows.iter().filter(|row| row.lead).count(), 1);
}

#[test]
fn behind_leads_with_pull_and_ahead_with_push() {
    let behind = RepoBadge {
        behind: 4,
        ..badge(BadgeKind::Behind)
    };
    let shown = model(&full(false), &behind);
    assert_eq!(shown.rows[0].choice, run(GitAction::Pull));
    assert_eq!(shown.rows[0].note, "4 commits behind");
    let ahead = RepoBadge {
        ahead: 1,
        ..badge(BadgeKind::Ahead)
    };
    let shown = model(&full(false), &ahead);
    assert_eq!(shown.rows[0].choice, run(GitAction::Push));
    assert_eq!(shown.rows[0].note, "1 commit ahead");
}

#[test]
fn a_clean_repository_disables_commit_and_leads_with_log() {
    let shown = model(&full(false), &badge(BadgeKind::Clean));
    let commit = row(&shown, GitAction::Commit);
    assert!(!commit.enabled);
    assert_eq!(commit.note, "Nothing to commit");
    assert_eq!(shown.rows[0].choice, run(GitAction::Log));
    assert_eq!(shown.rows[0].note, "main");
    assert!(!row(&shown, GitAction::Push).enabled);
}

#[test]
fn diverged_leads_with_pull_and_dims_push() {
    let diverged = RepoBadge {
        ahead: 2,
        behind: 5,
        ..badge(BadgeKind::Diverged)
    };
    let shown = model(&full(false), &diverged);
    assert_eq!(shown.rows[0].choice, run(GitAction::Pull));
    assert!(shown.rows[0].enabled);
    let push = row(&shown, GitAction::Push);
    assert!(!push.enabled);
    assert_eq!(push.note, "Pull first: histories diverged");
}

#[test]
fn the_command_line_cannot_pull_a_diverged_branch() {
    let diverged = RepoBadge {
        ahead: 2,
        behind: 5,
        ..badge(BadgeKind::Diverged)
    };
    let shown = model(&full(true), &diverged);
    let pull = row(&shown, GitAction::Pull);
    assert!(!pull.enabled);
    assert_eq!(pull.note, "Needs a merge: use a Git tool");
    assert_eq!(pull.confirm.as_deref(), Some("Pull main from origin"));
}

#[test]
fn a_fast_forward_pull_with_confirmation_says_so() {
    let behind = RepoBadge {
        behind: 1,
        ..badge(BadgeKind::Behind)
    };
    let shown = model(&full(true), &behind);
    assert_eq!(
        row(&shown, GitAction::Pull).note,
        "1 commit behind · fast-forward"
    );
}

#[test]
fn unknown_offers_check_again_first_and_blocks_the_writes() {
    let unknown = RepoBadge {
        reason: None,
        ..badge(BadgeKind::Unknown)
    };
    let shown = model(&full(false), &unknown);
    assert_eq!(shown.rows[0].choice, Choice::Recheck);
    assert_eq!(shown.rows[0].label, "Check again");
    for action in [GitAction::Commit, GitAction::Pull, GitAction::Push] {
        let blocked = row(&shown, action);
        assert!(!blocked.enabled, "{action:?}");
        assert_eq!(blocked.note, "Status unknown");
    }
    assert!(row(&shown, GitAction::Log).enabled);
}

#[test]
fn a_conflict_leads_with_diff_and_asks_to_resolve_first() {
    let conflict = RepoBadge {
        conflicted: 2,
        changed: 2,
        ..badge(BadgeKind::Conflict)
    };
    let shown = model(&full(false), &conflict);
    assert_eq!(shown.rows[0].choice, run(GitAction::Diff));
    assert_eq!(
        row(&shown, GitAction::Commit).note,
        "Resolve 2 conflicts first"
    );
    assert_eq!(row(&shown, GitAction::Pull).note, "Finish the merge first");
}

#[test]
fn diff_is_hidden_when_the_hooks_cannot_diff() {
    let source = Source {
        menu: &full(false),
        badge: &badge(BadgeKind::Clean),
        tool: Some("Fork"),
        can_diff: false,
        previews: None,
    };
    let shown = build(&source);
    assert!(!order(&shown).contains(&run(GitAction::Diff)));
    assert_eq!(shown.rows.len(), 4);
}

#[test]
fn an_open_only_tool_shows_just_open_in_the_tool() {
    let menu = GitMenu {
        head: "navo".to_owned(),
        items: vec![
            item(GitAction::Open, "Open in GitKraken", None),
            item(GitAction::Commit, "Commit…", None),
        ],
    };
    let shown = model(&menu, &badge(BadgeKind::Modified));
    assert_eq!(order(&shown), [run(GitAction::Open)]);
    assert_eq!(shown.rows[0].label, "Open in GitKraken");
    assert_eq!(shown.rows[0].note, "navo");
    assert_eq!(shown.via, Via::Tool("TortoiseGit".to_owned()));
}

#[test]
fn no_tool_offers_diff_then_choose_a_tool() {
    let menu = GitMenu {
        head: "navo".to_owned(),
        items: vec![
            item(GitAction::Diff, "Diff", None),
            item(GitAction::ChooseTool, "Choose a Git tool…", None),
        ],
    };
    let source = Source {
        menu: &menu,
        badge: &badge(BadgeKind::Clean),
        tool: None,
        can_diff: true,
        previews: None,
    };
    let shown = build(&source);
    assert_eq!(
        order(&shown),
        [run(GitAction::Diff), run(GitAction::ChooseTool)]
    );
    assert_eq!(shown.via, Via::NoTool);
    assert_eq!(shown.rows[1].note, "Settings → Git");
}

#[test]
fn a_subfolder_describes_its_own_changes_and_leaves_sync_notes_empty() {
    let below = RepoBadge {
        is_root: false,
        branch: None,
        upstream: None,
        changed: 12,
        ..badge(BadgeKind::Modified)
    };
    let shown = model(&full(false), &below);
    assert_eq!(row(&shown, GitAction::Commit).note, "12 changed below");
    assert_eq!(row(&shown, GitAction::Log).note, "this folder");
    assert_eq!(row(&shown, GitAction::Pull).note, "");
    assert!(row(&shown, GitAction::Push).enabled);
    assert_eq!(shown.head.place.as_deref(), Some("Inside a repository"));
}

#[test]
fn the_head_names_branch_state_and_fetch_time() {
    let fetched = RepoBadge {
        fetched_ms: Some(1_791_920_400_000),
        ahead: 2,
        changed: 1,
        ..badge(BadgeKind::Modified)
    };
    let shown = model(&full(false), &fetched);
    assert_eq!(shown.head.title, "navo");
    assert_eq!(shown.head.place.as_deref(), Some("main → origin/main"));
    assert_eq!(shown.head.state, "Modified");
    assert!(shown.head.facts.contains(&"2↑".to_owned()));
    assert!(
        shown
            .head
            .facts
            .iter()
            .any(|fact| fact.starts_with("fetched "))
    );
}

fn waiting() -> GitMenu {
    let mut menu = full(false);
    for item in &mut menu.items {
        item.pending = matches!(item.action, GitAction::Pull | GitAction::Push);
    }
    menu
}

fn behind_by_two() -> RepoBadge {
    RepoBadge {
        behind: 2,
        ..badge(BadgeKind::Behind)
    }
}

fn model_with(menu: &GitMenu, badge: &RepoBadge, previews: Option<&Previewed>) -> Model {
    build(&Source {
        menu,
        badge,
        tool: Some("Command line"),
        can_diff: true,
        previews,
    })
}

#[test]
fn pending_pull_and_push_are_disabled_without_a_confirmation() {
    let shown = model_with(&waiting(), &behind_by_two(), None);
    for action in [GitAction::Pull, GitAction::Push] {
        let held = row(&shown, action);
        assert!(!held.enabled && held.pending && held.confirm.is_none());
    }
    assert!(row(&shown, GitAction::Log).enabled);
    assert!(!row(&shown, GitAction::Log).pending);
}

#[test]
fn a_reply_enables_the_row_with_its_confirmation_and_keeps_the_rows() {
    let before = model_with(&waiting(), &behind_by_two(), None);
    let replies = vec![
        (GitAction::Pull, shown("Pull main from origin", "7")),
        (GitAction::Push, shown("Push main to origin", "8")),
    ];
    let after = model_with(&waiting(), &behind_by_two(), Some(&replies));
    let pull = row(&after, GitAction::Pull);
    assert!(pull.enabled && pull.settled && !pull.pending);
    assert_eq!(pull.confirm.as_deref(), Some("Pull main from origin"));
    assert_eq!(pull.confirm_id.as_deref(), Some("7"));
    assert_eq!(order(&before), order(&after));
}

#[test]
fn a_refused_preview_leaves_the_row_disabled_while_the_model_has_a_note() {
    let replies = vec![(GitAction::Pull, None), (GitAction::Push, None)];
    let shown = model_with(&waiting(), &behind_by_two(), Some(&replies));
    let pull = row(&shown, GitAction::Pull);
    assert!(!pull.enabled && !pull.pending && !pull.note.is_empty());
}

#[test]
fn a_refused_preview_without_a_note_removes_the_row() {
    let inside = RepoBadge {
        is_root: false,
        ..badge(BadgeKind::Clean)
    };
    let replies = vec![(GitAction::Pull, None)];
    let shown = model_with(&waiting(), &inside, Some(&replies));
    assert!(!order(&shown).contains(&run(GitAction::Pull)));
    assert!(!order(&shown).contains(&run(GitAction::Push)));
    assert!(order(&shown).contains(&run(GitAction::Commit)));
}
