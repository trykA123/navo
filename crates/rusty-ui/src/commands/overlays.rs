use super::Group;

pub(super) const GROUPS: [Group; 9] = [
    Group {
        context: "ContextMenu",
        commands: &[
            ("context_menu::MenuUp", "context_menu.up", "Up"),
            ("context_menu::MenuDown", "context_menu.down", "Down"),
            ("context_menu::MenuHome", "context_menu.first", "First item"),
            ("context_menu::MenuEnd", "context_menu.last", "Last item"),
            (
                "context_menu::MenuPageUp",
                "context_menu.page_up",
                "Page up",
            ),
            (
                "context_menu::MenuPageDown",
                "context_menu.page_down",
                "Page down",
            ),
            (
                "context_menu::MenuEnter",
                "context_menu.choose",
                "Choose item",
            ),
            (
                "context_menu::MenuRight",
                "context_menu.open_submenu",
                "Open submenu",
            ),
            (
                "context_menu::MenuLeft",
                "context_menu.close_submenu",
                "Close submenu",
            ),
            (
                "context_menu::MenuEscape",
                "context_menu.back",
                "Back or close",
            ),
            (
                "context_menu::MenuClassic",
                "context_menu.show_classic",
                "Classic menu",
            ),
        ],
    },
    Group {
        context: "AppMenu",
        commands: &[
            ("app_menu::FocusLeft", "app_menu.focus_left", "Left"),
            ("app_menu::FocusRight", "app_menu.focus_right", "Right"),
            ("app_menu::FocusUp", "app_menu.focus_up", "Up"),
            ("app_menu::FocusDown", "app_menu.focus_down", "Down"),
            ("app_menu::Activate", "app_menu.activate", "Choose item"),
            ("app_menu::CloseMenu", "app_menu.close", "Close app menu"),
        ],
    },
    Group {
        context: "SettingsSheet",
        commands: &[
            ("settings_sheet::RowUp", "settings.row_up", "Up"),
            ("settings_sheet::RowDown", "settings.row_down", "Down"),
            (
                "settings_sheet::ValueBack",
                "settings.previous_value",
                "Previous value",
            ),
            (
                "settings_sheet::ValueForward",
                "settings.next_value",
                "Next value",
            ),
            (
                "settings_sheet::ValueFlip",
                "settings.flip_value",
                "Turn on or off",
            ),
            (
                "settings_sheet::NextSection",
                "settings.next_section",
                "Next section",
            ),
            (
                "settings_sheet::PreviousSection",
                "settings.previous_section",
                "Previous section",
            ),
            (
                "settings_sheet::EditValue",
                "settings.edit_value",
                "Edit value",
            ),
            (
                "settings_sheet::OpenKey",
                "settings.show_in_file",
                "Show in settings file",
            ),
            ("settings_sheet::UndoChange", "settings.undo", "Undo change"),
            (
                "settings_sheet::CloseSettings",
                "settings.close",
                "Close settings",
            ),
        ],
    },
    Group {
        context: "SettingsField",
        commands: &[(
            "settings_sheet::CancelEdit",
            "settings.cancel_edit",
            "Cancel edit",
        )],
    },
    Group {
        context: "Shortcuts",
        commands: &[
            (
                "shortcuts::PreviousRow",
                "shortcuts.previous_row",
                "Previous shortcut",
            ),
            ("shortcuts::NextRow", "shortcuts.next_row", "Next shortcut"),
            ("shortcuts::Record", "shortcuts.record", "Record a new key"),
            (
                "shortcuts::Cancel",
                "shortcuts.cancel",
                "Cancel the new key",
            ),
            (
                "shortcuts::Reset",
                "shortcuts.reset",
                "Reset to the default key",
            ),
        ],
    },
    Group {
        context: "Dialog",
        commands: &[
            ("dialog_focus::Next", "dialog.next_field", "Next field"),
            (
                "dialog_focus::Previous",
                "dialog.previous_field",
                "Previous field",
            ),
        ],
    },
    Group {
        context: "DialogButton",
        commands: &[
            (
                "dialog_focus::NextButton",
                "dialog.next_button",
                "Next button",
            ),
            (
                "dialog_focus::PreviousButton",
                "dialog.previous_button",
                "Previous button",
            ),
            ("dialog_focus::Press", "dialog.press", "Press button"),
        ],
    },
    Group {
        context: "ConfirmDialog",
        commands: &[("confirm_dialog::Dismiss", "confirm.cancel", "Cancel")],
    },
    Group {
        context: "PromptDialog",
        commands: &[("prompt_dialog::Dismiss", "prompt.cancel", "Cancel")],
    },
];
