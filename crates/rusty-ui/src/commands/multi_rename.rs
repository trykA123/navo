use super::Group;

pub(super) const GROUPS: [Group; 2] = [
    Group {
        context: "MultiRename",
        commands: &[
            (
                "multi_rename::Rename",
                "multi_rename.rename",
                "cmd-multi-rename-rename",
            ),
            (
                "multi_rename::Close",
                "multi_rename.close",
                "cmd-multi-rename-close",
            ),
            (
                "multi_rename::TokenMenu",
                "multi_rename.token_menu",
                "cmd-multi-rename-token-menu",
            ),
            (
                "multi_rename::FocusPreview",
                "multi_rename.focus_preview",
                "cmd-multi-rename-focus-preview",
            ),
            (
                "multi_rename::FocusFields",
                "multi_rename.focus_fields",
                "cmd-multi-rename-focus-fields",
            ),
            (
                "multi_rename::PageUp",
                "multi_rename.page_up",
                "cmd-multi-rename-page-up",
            ),
            (
                "multi_rename::PageDown",
                "multi_rename.page_down",
                "cmd-multi-rename-page-down",
            ),
            (
                "multi_rename::NextProblem",
                "multi_rename.next_problem",
                "cmd-multi-rename-next-problem",
            ),
            (
                "multi_rename::PreviousProblem",
                "multi_rename.previous_problem",
                "cmd-multi-rename-previous-problem",
            ),
            (
                "multi_rename::SavePreset",
                "multi_rename.save_preset",
                "cmd-multi-rename-save-preset",
            ),
            (
                "multi_rename::OpenPresets",
                "multi_rename.open_presets",
                "cmd-multi-rename-open-presets",
            ),
            (
                "multi_rename::Reset",
                "multi_rename.reset",
                "cmd-multi-rename-reset",
            ),
        ],
    },
    Group {
        context: "MultiRenamePreview",
        commands: &[(
            "multi_rename::ToggleRow",
            "multi_rename.toggle_row",
            "cmd-multi-rename-toggle-row",
        )],
    },
];
