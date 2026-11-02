use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 2] = [
    Group {
        context: GLOBAL,
        commands: &[
            (
                "navo_window::OpenClipboard",
                "clipboard.open",
                "cmd-clipboard-open",
            ),
            (
                "navo_window::PauseClipboard",
                "clipboard.pause",
                "cmd-clipboard-pause",
            ),
            (
                "navo_window::ClearClipboardHistory",
                "clipboard.clear_history",
                "cmd-clipboard-clear-history",
            ),
            (
                "navo_window::DeleteClipboardData",
                "clipboard.delete_all",
                "cmd-clipboard-delete-all",
            ),
        ],
    },
    Group {
        context: "ClipPalette",
        commands: &[
            (
                "clip_palette::NextMode",
                "clip_palette.next_mode",
                "cmd-clip-palette-next-mode",
            ),
            (
                "clip_palette::PreviousMode",
                "clip_palette.previous_mode",
                "cmd-clip-palette-previous-mode",
            ),
            (
                "clip_palette::MoveUp",
                "clip_palette.up",
                "cmd-clip-palette-up",
            ),
            (
                "clip_palette::MoveDown",
                "clip_palette.down",
                "cmd-clip-palette-down",
            ),
            (
                "clip_palette::Choose",
                "clip_palette.choose",
                "cmd-clip-palette-choose",
            ),
            (
                "clip_palette::Back",
                "clip_palette.back",
                "cmd-clip-palette-back",
            ),
            (
                "clip_palette::OpenActions",
                "clip_palette.open_actions",
                "cmd-clip-palette-open-actions",
            ),
            (
                "clip_palette::CopyOnly",
                "clip_palette.copy_only",
                "cmd-clip-palette-copy-only",
            ),
            (
                "clip_palette::PasteFiles",
                "clip_palette.paste_files",
                "cmd-clip-palette-paste-files",
            ),
            (
                "clip_palette::ToggleReveal",
                "clip_palette.toggle_reveal",
                "cmd-clip-palette-toggle-reveal",
            ),
            (
                "clip_palette::CopyUser",
                "clip_palette.copy_user",
                "cmd-clip-palette-copy-user",
            ),
            (
                "clip_palette::SaveRow",
                "clip_palette.save_row",
                "cmd-clip-palette-save-row",
            ),
            (
                "clip_palette::DeleteRow",
                "clip_palette.delete_row",
                "cmd-clip-palette-delete-row",
            ),
            (
                "clip_palette::OpenManage",
                "clip_palette.manage",
                "cmd-clip-palette-manage",
            ),
        ],
    },
];

/// Whether `name` is one of the clipboard commands, which need clipboard hooks.
pub(super) fn needs_hooks(name: &str) -> bool {
    GROUPS[0]
        .commands
        .iter()
        .any(|&(_, command, _)| command == name)
}
