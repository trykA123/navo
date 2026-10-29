use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "NavoPane",
    commands: &[
        (
            "ferrite_tabs::TogglePinTab",
            "tab.toggle_pin",
            "cmd-tab-toggle-pin",
        ),
        (
            "ferrite_tabs::DuplicateTab",
            "tab.duplicate",
            "cmd-tab-duplicate",
        ),
        (
            "ferrite_tabs::CloseOtherTabs",
            "tab.close_others",
            "cmd-tab-close-others",
        ),
        (
            "ferrite_tabs::CloseTabsToRight",
            "tab.close_right",
            "cmd-tab-close-right",
        ),
        (
            "ferrite_tabs::ReopenClosedTab",
            "tab.reopen_closed",
            "cmd-tab-reopen-closed",
        ),
        (
            "ferrite_tabs::MoveTabToNewWorkspace",
            "tab.move_to_new_workspace",
            "cmd-tab-move-to-new-workspace",
        ),
        (
            "ferrite_tabs::MoveTabToNewPane",
            "tab.move_to_new_pane",
            "cmd-tab-move-to-new-pane",
        ),
        (
            "ferrite_tabs::CopyTabPath",
            "tab.copy_path",
            "cmd-tab-copy-path",
        ),
    ],
}];
