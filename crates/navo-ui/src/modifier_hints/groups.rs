//! Ribbon groups: the short group each command sits in, the group order per layer,
//! and which commands use the danger style.

use super::combo::Combo;

const GO: &str = "ribbon-group-go";
const MARKS: &str = "ribbon-group-marks";
const FILES: &str = "ribbon-group-files";
const TABS: &str = "ribbon-group-tabs";
const PANES: &str = "ribbon-group-panes";
const FILES_AND_PANES: &str = "ribbon-group-files-and-panes";
const WINDOW: &str = "ribbon-group-window";
const WORKSPACES: &str = "ribbon-group-workspaces";

/// Group order for every layer except Ctrl+Shift, which leads with its tabs.
const ORDER: [&str; 8] = [
    GO,
    MARKS,
    FILES,
    TABS,
    PANES,
    FILES_AND_PANES,
    WINDOW,
    WORKSPACES,
];
const CTRL_SHIFT_ORDER: [&str; 8] = [
    TABS,
    PANES,
    FILES,
    GO,
    MARKS,
    FILES_AND_PANES,
    WINDOW,
    WORKSPACES,
];

/// Commands per group, in the order they show; one list serves every layer.
const GROUPS: [(&str, &[&str]); 8] = [
    (GO, &["pane.go_back", "pane.go_forward", "pane.edit_path"]),
    (
        MARKS,
        &[
            "files.extend_up",
            "files.extend_down",
            "files.extend_page_up",
            "files.extend_page_down",
            "files.extend_first",
            "files.extend_last",
        ],
    ),
    (
        FILES,
        &[
            "files.clipboard_copy",
            "files.clipboard_cut",
            "files.paste",
            "files.undo",
            "files.mark_all",
            "files.unmark_all",
            "files.multi_rename",
            "files.refresh",
            "files.mark_extension",
            "files.unmark_extension",
            "files.context_menu",
            "git.menu",
        ],
    ),
    (
        TABS,
        &[
            "tab.new",
            "tab.close",
            "tab.reopen_closed",
            "tab.next",
            "tab.previous",
            "tab.clone",
            "tab.go_to_first",
            "tab.go_to_second",
            "tab.go_to_third",
            "tab.go_to_fourth",
            "tab.go_to_fifth",
            "tab.go_to_sixth",
            "tab.go_to_seventh",
            "tab.go_to_eighth",
            "tab.go_to_last",
        ],
    ),
    (
        PANES,
        &[
            "layout.split_right",
            "layout.split_below",
            "layout.close_pane",
            "layout.focus_pane_first",
            "layout.focus_pane_second",
            "layout.focus_pane_third",
            "layout.focus_pane_fourth",
            "pane.reset_split",
            "compare.with_other_pane",
            "preview.browse",
        ],
    ),
    (FILES_AND_PANES, &["files.delete", "pane.switch_back"]),
    (
        WINDOW,
        &[
            "app.command_palette",
            "jobs.toggle",
            "settings.toggle",
            "layout.switcher",
            "workspace.save",
            "sheet.narrower",
            "sheet.wider",
            "window.view_menu",
            "activity.focus",
            "workspace.new",
            "workspace.preview_first",
            "workspace.preview_second",
            "workspace.preview_third",
            "workspace.preview_fourth",
            "workspace.preview_fifth",
            "workspace.preview_sixth",
            "workspace.preview_seventh",
            "workspace.preview_eighth",
            "workspace.preview_ninth",
            "workspace.cancel_preview",
        ],
    ),
    (WORKSPACES, &["workspace.move_left", "workspace.move_right"]),
];

/// Commands that destroy data without Undo; the ribbon draws them in the mark colour.
const DANGER: [&str; 1] = ["files.delete"];

/// True when the command destroys data without Undo.
#[must_use]
pub fn is_danger(command: &str) -> bool {
    DANGER.contains(&command)
}

/// Picks a group for a command no list names (a user binding), by its prefix.
fn fallback(command: &str) -> &'static str {
    match command.split_once('.').map(|(prefix, _)| prefix) {
        Some("tab") => TABS,
        Some("pane" | "layout") => PANES,
        Some("files") => FILES,
        _ => WINDOW,
    }
}

/// Returns the l10n id of the command's group.
#[must_use]
pub fn group_of(command: &str) -> &'static str {
    GROUPS
        .iter()
        .find(|(_, commands)| commands.contains(&command))
        .map_or_else(|| fallback(command), |(group, _)| group)
}

/// Sort key of a command on a layer: its group's rank, then its place in the group
/// (unlisted commands last).
#[must_use]
pub fn place_of(command: &str, combo: Combo) -> (usize, usize) {
    let order = if combo == Combo::CTRL_SHIFT {
        &CTRL_SHIFT_ORDER
    } else {
        &ORDER
    };
    let group = group_of(command);
    let rank = order
        .iter()
        .position(|id| *id == group)
        .unwrap_or(order.len());
    let at = GROUPS
        .iter()
        .find_map(|(_, commands)| commands.iter().position(|name| *name == command))
        .unwrap_or(usize::MAX);
    (rank, at)
}
