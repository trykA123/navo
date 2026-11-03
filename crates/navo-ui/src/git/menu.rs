use navo_l10n::{t, tr};

use super::{BadgeKind, GitAction, GitMenu, GitMenuItem, Previewed, RepoBadge, Shown};
use crate::file_list::git_text;
use crate::format;
use crate::primitives::IconName;

/// What a menu row does when chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Run(GitAction),
    Recheck,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub choice: Choice,
    pub icon: IconName,
    pub label: String,
    pub note: String,
    pub enabled: bool,
    pub lead: bool,
    pub confirm: Option<String>,
    /// The id of the target `confirm` names; a run hands it back to the hooks.
    pub confirm_id: Option<String>,
    /// The confirmation is still being read; the row stays disabled.
    pub pending: bool,
    /// The confirmation arrived after the menu opened; the row fades in.
    pub settled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub title: String,
    pub place: Option<String>,
    pub state: String,
    pub facts: Vec<String>,
}

/// The footer's left side: the tool the menu runs through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    Tool(String),
    NoTool,
    Unnamed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub badge: RepoBadge,
    pub head: Head,
    pub rows: Vec<Row>,
    pub via: Via,
}

pub struct Source<'a> {
    pub menu: &'a GitMenu,
    pub badge: &'a RepoBadge,
    pub tool: Option<&'a str>,
    pub can_diff: bool,
    /// The replies to the pending items; `None` while they are still being read.
    pub previews: Option<&'a Previewed>,
}

struct Facts {
    unknown: bool,
    conflict: bool,
    is_root: bool,
    changed: u32,
    untracked: u32,
    conflicted: u32,
    ahead: u32,
    behind: u32,
}

impl Facts {
    fn of(badge: &RepoBadge) -> Self {
        Self {
            unknown: matches!(badge.kind, BadgeKind::Unknown | BadgeKind::Checking),
            conflict: badge.kind == BadgeKind::Conflict,
            is_root: badge.is_root,
            changed: badge.changed.saturating_add(badge.untracked),
            untracked: badge.untracked,
            conflicted: badge.conflicted,
            ahead: badge.ahead,
            behind: badge.behind,
        }
    }

    fn blocked(&self) -> bool {
        self.unknown || self.conflict
    }

    fn diverged(&self) -> bool {
        self.ahead > 0 && self.behind > 0
    }
}

const ORDER: [GitAction; 5] = [
    GitAction::Commit,
    GitAction::Pull,
    GitAction::Push,
    GitAction::Log,
    GitAction::Diff,
];

fn icon_of(action: GitAction) -> IconName {
    match action {
        GitAction::Commit => IconName::GitCommitHorizontal,
        GitAction::Pull => IconName::ArrowDownToLine,
        GitAction::Push => IconName::ArrowUpFromLine,
        GitAction::Log => IconName::RotateCcwClock,
        GitAction::Diff => IconName::Diff,
        GitAction::Open => IconName::ExternalLink,
        GitAction::ChooseTool => IconName::Wrench,
    }
}

fn is_enabled(action: GitAction, f: &Facts, guarded: bool) -> bool {
    match action {
        GitAction::Commit => !f.blocked() && f.changed > 0,
        GitAction::Pull => !f.blocked() && !(guarded && f.diverged()),
        GitAction::Push => !f.blocked() && (!f.is_root || (f.ahead > 0 && f.behind == 0)),
        _ => true,
    }
}

fn blocked_note(f: &Facts, resolve: bool) -> Option<String> {
    match (f.unknown, f.conflict) {
        (true, _) => Some(tr("git-menu-unknown")),
        (_, true) if resolve => Some(t!("git-menu-resolve", count = f.conflicted)),
        (_, true) => Some(tr("git-menu-finish-merge")),
        _ => None,
    }
}

fn commit_note(f: &Facts) -> String {
    match (f.changed, f.is_root, f.untracked) {
        (0, ..) => tr("git-menu-nothing-to-commit"),
        (n, false, _) => t!("git-detail-changed-below", count = n),
        (n, true, 0) => t!("git-menu-changed", count = n),
        (n, true, new) => t!("git-menu-changed-new", count = n, new = new),
    }
}

fn pull_note(f: &Facts, guarded: bool) -> String {
    match (f.is_root, f.diverged(), f.behind) {
        (false, ..) => String::new(),
        (_, true, _) if guarded => tr("git-menu-needs-merge"),
        (_, true, _) => tr("git-menu-diverged"),
        (_, _, 0) => tr("git-menu-fetch-merge"),
        (_, _, n) if guarded => t!("git-menu-behind-ff", count = n),
        (_, _, n) => t!("git-menu-behind", count = n),
    }
}

fn push_note(f: &Facts) -> String {
    match (f.is_root, f.diverged(), f.ahead) {
        (false, ..) => String::new(),
        (_, true, _) => tr("git-menu-pull-first"),
        (_, _, 0) => tr("git-menu-nothing-to-push"),
        (_, _, n) => t!("git-menu-ahead", count = n),
    }
}

fn note_of(action: GitAction, f: &Facts, source: &Source, guarded: bool) -> String {
    let block = blocked_note(f, action == GitAction::Commit);
    match action {
        GitAction::Commit => block.unwrap_or_else(|| commit_note(f)),
        GitAction::Pull => block.unwrap_or_else(|| pull_note(f, guarded)),
        GitAction::Push => block.unwrap_or_else(|| push_note(f)),
        GitAction::Log if !f.is_root => tr("git-menu-this-folder"),
        GitAction::Log => source.badge.branch.clone().unwrap_or_default(),
        GitAction::Diff if !f.is_root => tr("git-menu-this-folder"),
        GitAction::Diff => tr("git-menu-working-tree"),
        GitAction::ChooseTool => tr("git-menu-choose-note"),
        GitAction::Open => source.menu.head.clone(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preview<'a> {
    NotNeeded,
    Waiting,
    Ready(&'a Shown),
    Refused,
}

fn preview_of<'a>(item: &GitMenuItem, source: &Source<'a>) -> Preview<'a> {
    if !item.pending {
        return Preview::NotNeeded;
    }
    let Some(replies) = source.previews else {
        return Preview::Waiting;
    };
    match replies.iter().find(|(action, _)| *action == item.action) {
        Some((_, Some(shown))) => Preview::Ready(shown),
        _ => Preview::Refused,
    }
}

fn row_of(item: &GitMenuItem, f: &Facts, source: &Source) -> Row {
    let preview = preview_of(item, source);
    let (confirm, confirm_id) = match preview {
        Preview::Ready(shown) => (Some(shown.text.clone()), Some(shown.id.clone())),
        _ => (item.confirm.clone(), item.confirm_id.clone()),
    };
    let guarded = confirm.is_some();
    let held = matches!(preview, Preview::Waiting | Preview::Refused);
    Row {
        choice: Choice::Run(item.action),
        icon: icon_of(item.action),
        label: item.label.clone(),
        note: note_of(item.action, f, source, guarded),
        enabled: !held && is_enabled(item.action, f, guarded),
        lead: false,
        confirm,
        confirm_id,
        pending: preview == Preview::Waiting,
        settled: matches!(preview, Preview::Ready(_)),
    }
}

fn lead_of(f: &Facts, can_diff: bool) -> Choice {
    if f.unknown {
        return Choice::Recheck;
    }
    Choice::Run(if f.conflict {
        if can_diff {
            GitAction::Diff
        } else {
            GitAction::Log
        }
    } else if f.changed > 0 {
        GitAction::Commit
    } else if f.behind > 0 {
        GitAction::Pull
    } else if f.ahead > 0 {
        GitAction::Push
    } else {
        GitAction::Log
    })
}

fn recheck_row() -> Row {
    Row {
        choice: Choice::Recheck,
        icon: IconName::RefreshCw,
        label: tr("git-menu-recheck"),
        note: String::new(),
        enabled: true,
        lead: false,
        confirm: None,
        confirm_id: None,
        pending: false,
        settled: false,
    }
}

fn find_item(menu: &GitMenu, action: GitAction) -> Option<&GitMenuItem> {
    menu.items.iter().find(|item| item.action == action)
}

fn open_only(source: &Source, f: &Facts) -> Option<Vec<Row>> {
    let item = find_item(source.menu, GitAction::Open)?;
    let mut row = row_of(item, f, source);
    row.lead = true;
    Some(vec![row])
}

fn is_refused_and_unexplained(item: &GitMenuItem, row: &Row, source: &Source) -> bool {
    preview_of(item, source) == Preview::Refused && row.note.is_empty()
}

fn tool_rows(source: &Source, f: &Facts) -> Vec<Row> {
    let mut rows: Vec<Row> = ORDER
        .into_iter()
        .filter(|action| *action != GitAction::Diff || source.can_diff)
        .filter_map(|action| find_item(source.menu, action))
        .map(|item| (item, row_of(item, f, source)))
        .filter(|(item, row)| !is_refused_and_unexplained(item, row, source))
        .map(|(_, row)| row)
        .collect();
    if f.unknown {
        rows.insert(0, recheck_row());
    }
    let lead = lead_of(f, source.can_diff);
    let at = rows.iter().position(|row| row.choice == lead).unwrap_or(0);
    if !rows.is_empty() {
        let first = rows.remove(at);
        rows.insert(0, first);
        rows[0].lead = true;
    }
    rows.extend(find_item(source.menu, GitAction::ChooseTool).map(|item| row_of(item, f, source)));
    rows
}

fn place_of(badge: &RepoBadge) -> Option<String> {
    match (&badge.branch, &badge.upstream, badge.is_root) {
        (_, _, false) => Some(tr("git-inside")),
        (Some(branch), Some(upstream), _) => Some(t!(
            "git-branch-upstream",
            branch = branch.as_str(),
            upstream = upstream.as_str()
        )),
        (Some(branch), None, _) => Some(branch.clone()),
        (None, ..) => None,
    }
}

fn facts_of(badge: &RepoBadge) -> Vec<String> {
    let counts = git_text::counts(badge).filter(|_| {
        !matches!(
            badge.kind,
            BadgeKind::Ahead | BadgeKind::Behind | BadgeKind::Diverged
        )
    });
    let fetched = badge
        .fetched_ms
        .filter(|_| git_text::is_settled(badge))
        .map(|at| t!("git-menu-fetched", time = format::read_time(at)));
    git_text::detail(badge)
        .into_iter()
        .chain(counts)
        .chain(fetched)
        .collect()
}

fn head_of(menu: &GitMenu, badge: &RepoBadge) -> Head {
    Head {
        title: menu.head.clone(),
        place: place_of(badge),
        state: tr(git_text::state_id(badge.kind)),
        facts: facts_of(badge),
    }
}

fn via_of(source: &Source) -> Via {
    match (source.tool, find_item(source.menu, GitAction::ChooseTool)) {
        (Some(tool), _) => Via::Tool(tool.to_owned()),
        (None, Some(_)) => Via::NoTool,
        (None, None) => Via::Unnamed,
    }
}

/// Builds the menu from the hooks' items and the badge: state-aware order, per-item facts and
/// enablement, Diff only when the hooks can diff, and Open alone for open-only tools.
pub fn build(source: &Source) -> Model {
    let f = Facts::of(source.badge);
    let rows = open_only(source, &f).unwrap_or_else(|| tool_rows(source, &f));
    Model {
        badge: source.badge.clone(),
        head: head_of(source.menu, source.badge),
        rows,
        via: via_of(source),
    }
}
