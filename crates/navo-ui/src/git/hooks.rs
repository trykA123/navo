use std::collections::HashMap;
use std::sync::Arc;

use navo_core::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Clean,
    Modified,
    Ahead,
    Behind,
    Diverged,
    Conflict,
    Unknown,
    Checking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeReason {
    TimedOut,
    NoGit,
    NotTrusted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoBadge {
    pub kind: BadgeKind,
    pub is_root: bool,
    pub stale: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub fetched_ms: Option<i64>,
    pub reason: Option<BadgeReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderBadges {
    pub revision: u64,
    /// The scanner gave up on this listing because its drive answered too slowly.
    pub slow_drive: bool,
    pub by_name: HashMap<String, RepoBadge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAction {
    Commit,
    Pull,
    Push,
    Log,
    Diff,
    Open,
    ChooseTool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitMenuItem {
    pub action: GitAction,
    pub label: String,
    pub confirm: Option<String>,
    /// The hooks' id for exactly the target `confirm` names; never shown.
    pub confirm_id: Option<String>,
    /// The confirmation is still being read; `GitHooks::previews` delivers it.
    pub pending: bool,
}

/// A confirmation the hooks read: the text the user sees and the single-use id of its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub id: String,
}

/// The confirmation per Pull or Push; `None` when the hooks could not read one.
pub type Previewed = Vec<(GitAction, Option<Shown>)>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitMenu {
    pub head: String,
    pub items: Vec<GitMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStarted {
    pub tool: String,
    /// The finished result for the status line, when the hooks ran the action themselves.
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRow {
    pub id: String,
    pub label: String,
    /// The detected program; empty when the tool has none, like the custom command.
    pub path: String,
    pub selected: bool,
}

pub trait GitHooks: Send + Sync {
    fn observe(&self, listing: &Location, revision: u64, names: Vec<String>);
    fn forget(&self, listing: &Location);
    fn badges(&self, listing: &Location) -> Option<Arc<FolderBadges>>;
    fn subscribe(&self, notify: Box<dyn Fn() + Send + Sync>);
    fn rescan(&self, listing: &Location);
    /// The window became active again. Hooks may rescan stale folders.
    fn focus_regained(&self) {}
    /// Answers at once from memory: no process, no disk. Slow parts arrive through `previews`.
    fn menu(&self, repo: &Location) -> GitMenu;
    /// Reads the confirmations of the menu's pending items off the calling thread and calls
    /// `done` once, from any thread. Requests for one repository may share one read.
    fn previews(&self, _repo: &Location, done: Box<dyn FnOnce(Previewed) + Send>) {
        done(Vec::new());
    }
    fn run(&self, repo: &Location, action: GitAction) -> Result<GitStarted, String>;
    /// `run` for a menu row: `shown` is the `GitMenuItem::confirm` text the user confirmed and
    /// `confirm_id` its id. Hooks refuse when they are not the confirmation they still hold.
    #[expect(
        clippy::too_many_arguments,
        reason = "a row's text and id are each optional, as `GitMenuItem` holds them"
    )]
    fn run_confirmed(
        &self,
        repo: &Location,
        action: GitAction,
        _shown: Option<&str>,
        _confirm_id: Option<&str>,
    ) -> Result<GitStarted, String> {
        self.run(repo, action)
    }
    fn tools(&self) -> Vec<ToolRow>;
    /// Why `template` cannot be the command of the Other tool; reads no disk and starts nothing.
    fn check_command(&self, _template: &str) -> Result<(), String> {
        Ok(())
    }
    /// Settings → Git opened: detect the tools again off the calling thread, then notify.
    fn refresh_tools(&self) {}
    fn can_diff(&self, repo: &Location) -> bool;
    // TODO: once Part U defines `compare.open-file-pair { left, right }`, build two Locations
    // (navo_fs::git::location(repo, "HEAD" or last-touching commit, inner), plus the local one) and dispatch it.
    fn open_diff(&self, repo: &Location, path: &Location) -> Result<GitStarted, String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoGit;

impl GitHooks for NoGit {
    fn observe(&self, _listing: &Location, _revision: u64, _names: Vec<String>) {}
    fn forget(&self, _listing: &Location) {}
    fn badges(&self, _listing: &Location) -> Option<Arc<FolderBadges>> {
        None
    }
    fn subscribe(&self, _notify: Box<dyn Fn() + Send + Sync>) {}
    fn rescan(&self, _listing: &Location) {}
    fn menu(&self, _repo: &Location) -> GitMenu {
        GitMenu::default()
    }
    fn run(&self, _repo: &Location, _action: GitAction) -> Result<GitStarted, String> {
        Err(navo_l10n::tr("git-off"))
    }
    fn tools(&self) -> Vec<ToolRow> {
        Vec::new()
    }
    fn can_diff(&self, _repo: &Location) -> bool {
        false
    }
    fn open_diff(&self, _repo: &Location, _path: &Location) -> Result<GitStarted, String> {
        Err(navo_l10n::tr("git-off"))
    }
}
