use super::Group;

pub(super) const GROUPS: [Group; 13] = [
    Group {
        context: "ContextMenu",
        commands: &[
            (
                "context_menu::MenuUp",
                "context_menu.up",
                "cmd-context-menu-up",
            ),
            (
                "context_menu::MenuDown",
                "context_menu.down",
                "cmd-context-menu-down",
            ),
            (
                "context_menu::MenuHome",
                "context_menu.first",
                "cmd-context-menu-first",
            ),
            (
                "context_menu::MenuEnd",
                "context_menu.last",
                "cmd-context-menu-last",
            ),
            (
                "context_menu::MenuPageUp",
                "context_menu.page_up",
                "cmd-context-menu-page-up",
            ),
            (
                "context_menu::MenuPageDown",
                "context_menu.page_down",
                "cmd-context-menu-page-down",
            ),
            (
                "context_menu::MenuEnter",
                "context_menu.choose",
                "cmd-context-menu-choose",
            ),
            (
                "context_menu::MenuRight",
                "context_menu.open_submenu",
                "cmd-context-menu-open-submenu",
            ),
            (
                "context_menu::MenuLeft",
                "context_menu.close_submenu",
                "cmd-context-menu-close-submenu",
            ),
            (
                "context_menu::MenuEscape",
                "context_menu.back",
                "cmd-context-menu-back",
            ),
            (
                "context_menu::MenuClassic",
                "context_menu.show_classic",
                "cmd-context-menu-show-classic",
            ),
        ],
    },
    Group {
        context: "CommandCenter",
        commands: &[
            (
                "command_center::Up",
                "command_center.up",
                "cmd-command-center-up",
            ),
            (
                "command_center::Down",
                "command_center.down",
                "cmd-command-center-down",
            ),
            (
                "command_center::PageUp",
                "command_center.page_up",
                "cmd-command-center-page-up",
            ),
            (
                "command_center::PageDown",
                "command_center.page_down",
                "cmd-command-center-page-down",
            ),
            (
                "command_center::First",
                "command_center.first",
                "cmd-command-center-first",
            ),
            (
                "command_center::Last",
                "command_center.last",
                "cmd-command-center-last",
            ),
            (
                "command_center::NextGroup",
                "command_center.next_group",
                "cmd-command-center-next-group",
            ),
            (
                "command_center::PreviousGroup",
                "command_center.previous_group",
                "cmd-command-center-previous-group",
            ),
            (
                "command_center::Run",
                "command_center.run",
                "cmd-command-center-run",
            ),
            (
                "command_center::RunInNewTab",
                "command_center.run_in_new_tab",
                "cmd-command-center-run-in-new-tab",
            ),
            (
                "command_center::Reveal",
                "command_center.reveal",
                "cmd-command-center-reveal",
            ),
            (
                "command_center::OtherPane",
                "command_center.other_pane",
                "cmd-command-center-other-pane",
            ),
            (
                "command_center::CopyPath",
                "command_center.copy_path",
                "cmd-command-center-copy-path",
            ),
            (
                "command_center::Back",
                "command_center.back",
                "cmd-command-center-back",
            ),
        ],
    },
    Group {
        context: "AppMenu",
        commands: &[
            (
                "app_menu::FocusLeft",
                "app_menu.focus_left",
                "cmd-app-menu-focus-left",
            ),
            (
                "app_menu::FocusRight",
                "app_menu.focus_right",
                "cmd-app-menu-focus-right",
            ),
            (
                "app_menu::FocusUp",
                "app_menu.focus_up",
                "cmd-app-menu-focus-up",
            ),
            (
                "app_menu::FocusDown",
                "app_menu.focus_down",
                "cmd-app-menu-focus-down",
            ),
            (
                "app_menu::Activate",
                "app_menu.activate",
                "cmd-app-menu-activate",
            ),
            (
                "app_menu::CloseMenu",
                "app_menu.close",
                "cmd-app-menu-close",
            ),
        ],
    },
    Group {
        context: "SettingsSheet",
        commands: &[
            (
                "settings_sheet::RowUp",
                "settings.row_up",
                "cmd-settings-row-up",
            ),
            (
                "settings_sheet::RowDown",
                "settings.row_down",
                "cmd-settings-row-down",
            ),
            (
                "settings_sheet::ValueBack",
                "settings.previous_value",
                "cmd-settings-previous-value",
            ),
            (
                "settings_sheet::ValueForward",
                "settings.next_value",
                "cmd-settings-next-value",
            ),
            (
                "settings_sheet::ValueFlip",
                "settings.flip_value",
                "cmd-settings-flip-value",
            ),
            (
                "settings_sheet::NextSection",
                "settings.next_section",
                "cmd-settings-next-section",
            ),
            (
                "settings_sheet::PreviousSection",
                "settings.previous_section",
                "cmd-settings-previous-section",
            ),
            (
                "settings_sheet::EditValue",
                "settings.edit_value",
                "cmd-settings-edit-value",
            ),
            (
                "settings_sheet::OpenKey",
                "settings.show_in_file",
                "cmd-settings-show-in-file",
            ),
            (
                "settings_sheet::ResetValue",
                "settings.reset_value",
                "cmd-settings-reset-value",
            ),
            (
                "settings_sheet::UndoChange",
                "settings.undo",
                "cmd-settings-undo",
            ),
            (
                "settings_sheet::FindSetting",
                "settings.find",
                "cmd-settings-find",
            ),
            (
                "settings_sheet::CloseSettings",
                "settings.close",
                "cmd-settings-close",
            ),
        ],
    },
    Group {
        context: "SettingsField",
        commands: &[(
            "settings_sheet::CancelEdit",
            "settings.cancel_edit",
            "cmd-settings-cancel-edit",
        )],
    },
    Group {
        context: "SettingsFind",
        commands: &[
            (
                "settings_sheet::ClearFind",
                "settings.clear_find",
                "cmd-settings-clear-find",
            ),
            (
                "settings_sheet::FindToList",
                "settings.find_to_list",
                "cmd-settings-find-to-list",
            ),
        ],
    },
    Group {
        context: "Shortcuts",
        commands: &[
            (
                "shortcuts::PreviousRow",
                "shortcuts.previous_row",
                "cmd-shortcuts-previous-row",
            ),
            (
                "shortcuts::NextRow",
                "shortcuts.next_row",
                "cmd-shortcuts-next-row",
            ),
            (
                "shortcuts::Record",
                "shortcuts.record",
                "cmd-shortcuts-record",
            ),
            (
                "shortcuts::Cancel",
                "shortcuts.cancel",
                "cmd-shortcuts-cancel",
            ),
            ("shortcuts::Reset", "shortcuts.reset", "cmd-shortcuts-reset"),
        ],
    },
    Group {
        context: "Dialog",
        commands: &[
            (
                "dialog_focus::Next",
                "dialog.next_field",
                "cmd-dialog-next-field",
            ),
            (
                "dialog_focus::Previous",
                "dialog.previous_field",
                "cmd-dialog-previous-field",
            ),
        ],
    },
    Group {
        context: "DialogButton",
        commands: &[
            (
                "dialog_focus::NextButton",
                "dialog.next_button",
                "cmd-dialog-next-button",
            ),
            (
                "dialog_focus::PreviousButton",
                "dialog.previous_button",
                "cmd-dialog-previous-button",
            ),
            ("dialog_focus::Press", "dialog.press", "cmd-dialog-press"),
        ],
    },
    Group {
        context: "ConfirmDialog",
        commands: &[(
            "confirm_dialog::Dismiss",
            "confirm.cancel",
            "cmd-confirm-cancel",
        )],
    },
    Group {
        context: "PromptDialog",
        commands: &[(
            "prompt_dialog::Dismiss",
            "prompt.cancel",
            "cmd-prompt-cancel",
        )],
    },
    Group {
        context: "ActivityStrip",
        commands: &[
            (
                "activity_strip::Previous",
                "activity.previous",
                "cmd-activity-previous",
            ),
            ("activity_strip::Next", "activity.next", "cmd-activity-next"),
            (
                "activity_strip::PauseOrResume",
                "activity.pause",
                "cmd-activity-pause",
            ),
            (
                "activity_strip::Remove",
                "activity.remove",
                "cmd-activity-remove",
            ),
            ("activity_strip::Press", "activity.act", "cmd-activity-act"),
            (
                "activity_strip::Leave",
                "activity.leave",
                "cmd-activity-leave",
            ),
        ],
    },
    Group {
        context: "ActivityDrawer",
        commands: &[
            (
                "activity_strip::PreviousItem",
                "activity.previous_item",
                "cmd-activity-previous-item",
            ),
            (
                "activity_strip::NextItem",
                "activity.next_item",
                "cmd-activity-next-item",
            ),
        ],
    },
];
