use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "NavoPane",
    commands: &[
        (
            "navo_tabs::TogglePinTab",
            "tab.toggle_pin",
            "cmd-tab-toggle-pin",
        ),
        (
            "navo_tabs::DuplicateTab",
            "tab.duplicate",
            "cmd-tab-duplicate",
        ),
        (
            "navo_tabs::CloseOtherTabs",
            "tab.close_others",
            "cmd-tab-close-others",
        ),
        (
            "navo_tabs::CloseTabsToRight",
            "tab.close_right",
            "cmd-tab-close-right",
        ),
        (
            "navo_tabs::ReopenClosedTab",
            "tab.reopen_closed",
            "cmd-tab-reopen-closed",
        ),
        (
            "navo_tabs::MoveTabToNewWorkspace",
            "tab.move_to_new_workspace",
            "cmd-tab-move-to-new-workspace",
        ),
        (
            "navo_tabs::MoveTabToNewPane",
            "tab.move_to_new_pane",
            "cmd-tab-move-to-new-pane",
        ),
        (
            "navo_tabs::CopyTabPath",
            "tab.copy_path",
            "cmd-tab-copy-path",
        ),
    ],
}];
