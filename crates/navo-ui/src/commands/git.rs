use super::Group;
use crate::git::GitAction;

pub(crate) const MENU: &str = "git.menu";

const NAMES: [(GitAction, &str); 5] = [
    (GitAction::Commit, "git.commit"),
    (GitAction::Pull, "git.pull"),
    (GitAction::Push, "git.push"),
    (GitAction::Log, "git.log"),
    (GitAction::Diff, "git.diff"),
];

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "FileList",
    commands: &[
        ("file_list::OpenGitMenu", MENU, "cmd-git-menu"),
        ("file_list::GitCommit", "git.commit", "cmd-git-commit"),
        ("file_list::GitPull", "git.pull", "cmd-git-pull"),
        ("file_list::GitPush", "git.push", "cmd-git-push"),
        ("file_list::GitLog", "git.log", "cmd-git-log"),
        ("file_list::GitDiff", "git.diff", "cmd-git-diff"),
    ],
}];

/// Commands that appear only on a repository row.
pub(crate) fn is_git(name: &str) -> bool {
    name == MENU || NAMES.iter().any(|&(_, command)| command == name)
}

pub(crate) fn name_of(action: GitAction) -> Option<&'static str> {
    NAMES
        .iter()
        .find_map(|&(at, name)| (at == action).then_some(name))
}
