use navo_l10n::{t, tr};
use navo_ui::git::{GitAction, GitMenuItem};

use super::tools::{Capability, DetectedTool, ToolKind};

const ACTIONS: [(GitAction, Capability, &str); 5] = [
    (GitAction::Commit, Capability::Commit, "git-item-commit"),
    (GitAction::Pull, Capability::Pull, "git-item-pull"),
    (GitAction::Push, Capability::Push, "git-item-push"),
    (GitAction::Log, Capability::Log, "git-item-log"),
    (GitAction::Diff, Capability::Diff, "git-item-diff"),
];

fn item(action: GitAction, label: String) -> GitMenuItem {
    GitMenuItem {
        action,
        label,
        confirm: None,
        confirm_id: None,
        pending: false,
    }
}

pub(super) fn can_diff(tool: Option<&DetectedTool>) -> bool {
    tool.is_some_and(|tool| tool.kind.capabilities().contains(&Capability::Diff))
}

/// The D100h items for the resolved tool. A git-cli Pull or Push waits for its confirmation.
pub(super) fn items(tool: Option<&DetectedTool>) -> Vec<GitMenuItem> {
    let choose = item(GitAction::ChooseTool, tr("git-item-choose"));
    let Some(tool) = tool else {
        return vec![choose];
    };
    let caps = tool.kind.capabilities();
    if caps == [Capability::OpenOnly] {
        let label = t!("git-item-open", tool = tool.label.clone());
        return vec![item(GitAction::Open, label)];
    }
    let waits = |action| {
        tool.kind == ToolKind::GitCli && matches!(action, GitAction::Pull | GitAction::Push)
    };
    let mut items: Vec<_> = ACTIONS
        .into_iter()
        .filter(|(_, capability, _)| caps.contains(capability))
        .map(|(action, _, id)| GitMenuItem {
            pending: waits(action),
            ..item(action, tr(id))
        })
        .collect();
    items.push(choose);
    items
}
