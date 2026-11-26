use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 3] = [
    Group {
        context: GLOBAL,
        commands: &[
            (
                "navo_window::FocusButtonBar",
                "button_bar.focus",
                "cmd-button-bar-focus",
            ),
            (
                "navo_window::EditButtonBar",
                "button_bar.edit",
                "cmd-button-bar-edit",
            ),
        ],
    },
    Group {
        context: "ButtonBar",
        commands: &[
            (
                "navo_window::ButtonBarPrevious",
                "button_bar.previous",
                "cmd-button-bar-previous",
            ),
            (
                "navo_window::ButtonBarNext",
                "button_bar.next",
                "cmd-button-bar-next",
            ),
            (
                "navo_window::ButtonBarRun",
                "button_bar.run",
                "cmd-button-bar-run",
            ),
            (
                "navo_window::ButtonBarOpenMenu",
                "button_bar.open_menu",
                "cmd-button-bar-open-menu",
            ),
            (
                "navo_window::ButtonBarContextMenu",
                "button_bar.context_menu",
                "cmd-button-bar-context-menu",
            ),
            (
                "navo_window::ButtonBarLeave",
                "button_bar.leave",
                "cmd-button-bar-leave",
            ),
        ],
    },
    Group {
        context: "ButtonBarEditor",
        commands: &[
            (
                "navo_window::ButtonBarEditorMoveLeft",
                "button_bar_editor.move_left",
                "cmd-button-bar-editor-move-left",
            ),
            (
                "navo_window::ButtonBarEditorMoveRight",
                "button_bar_editor.move_right",
                "cmd-button-bar-editor-move-right",
            ),
            (
                "navo_window::ButtonBarEditorAdd",
                "button_bar_editor.add",
                "cmd-button-bar-editor-add",
            ),
            (
                "navo_window::ButtonBarEditorRemove",
                "button_bar_editor.remove",
                "cmd-button-bar-editor-remove",
            ),
            (
                "navo_window::ButtonBarEditorDuplicate",
                "button_bar_editor.duplicate",
                "cmd-button-bar-editor-duplicate",
            ),
            (
                "navo_window::ButtonBarEditorUndo",
                "button_bar_editor.undo",
                "cmd-button-bar-editor-undo",
            ),
            (
                "navo_window::ButtonBarEditorDone",
                "button_bar_editor.done",
                "cmd-button-bar-editor-done",
            ),
        ],
    },
];
