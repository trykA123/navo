/// Context of the tiling layout mode keys.
pub const CONTEXT: &str = "LayoutMode";

/// Verbs of the layout mode: no action, the mode reads their keys itself.
pub(super) const COMMANDS: [(&str, &str); 20] = [
    ("layout_mode.split_right", "cmd-layout-mode-split-right"),
    ("layout_mode.split_below", "cmd-layout-mode-split-below"),
    ("layout_mode.close", "cmd-layout-mode-close"),
    ("layout_mode.swap_left", "cmd-layout-mode-swap-left"),
    ("layout_mode.swap_right", "cmd-layout-mode-swap-right"),
    ("layout_mode.swap_up", "cmd-layout-mode-swap-up"),
    ("layout_mode.swap_down", "cmd-layout-mode-swap-down"),
    ("layout_mode.resize_left", "cmd-layout-mode-resize-left"),
    ("layout_mode.resize_right", "cmd-layout-mode-resize-right"),
    ("layout_mode.resize_up", "cmd-layout-mode-resize-up"),
    ("layout_mode.resize_down", "cmd-layout-mode-resize-down"),
    ("layout_mode.even", "cmd-layout-mode-even"),
    ("layout_mode.save", "cmd-layout-mode-save"),
    ("layout_mode.remove", "cmd-layout-mode-remove"),
    ("layout_mode.target", "cmd-layout-mode-target"),
    (
        "layout_mode.focus_pane_first",
        "cmd-layout-mode-focus-pane-first",
    ),
    (
        "layout_mode.focus_pane_second",
        "cmd-layout-mode-focus-pane-second",
    ),
    (
        "layout_mode.focus_pane_third",
        "cmd-layout-mode-focus-pane-third",
    ),
    (
        "layout_mode.focus_pane_fourth",
        "cmd-layout-mode-focus-pane-fourth",
    ),
    ("layout_mode.done", "cmd-layout-mode-done"),
];

/// True for a bare digit 1-9 (a normalised key): it always picks a numbered layout (D5).
#[must_use]
pub fn is_fixed_digit(key: &str) -> bool {
    matches!(key.as_bytes(), [b'1'..=b'9'])
}

/// False for a bare digit 1-9 on a layout verb: the digit never reaches the verb, so no screen shows it.
#[must_use]
pub fn is_shown_key(command: &str, key: &str) -> bool {
    !(is_mode_command(command) && super::normal_key(key).is_some_and(|key| is_fixed_digit(&key)))
}

pub(super) fn is_mode_command(name: &str) -> bool {
    COMMANDS.iter().any(|&(command, _)| command == name)
}
