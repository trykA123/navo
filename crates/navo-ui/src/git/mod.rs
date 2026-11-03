mod hooks;
pub mod menu;
#[cfg(test)]
mod menu_tests;
pub mod menu_view;
#[cfg(test)]
mod tests;

pub use crate::file_list::git_cell::marked_icon;
pub use hooks::{
    BadgeKind, BadgeReason, FolderBadges, GitAction, GitHooks, GitMenu, GitMenuItem, GitStarted,
    NoGit, Previewed, RepoBadge, RepoRoot, Shown, ToolRow,
};
