use gpui_kit::KeyBinding;

use crate::prelude::*;

pub(super) const PANE: &str = "FerritePane";
pub(super) const WAITING: &str = "PaneWaiting";

gpui_kit::actions!(
    ferrite_window,
    [
        DriveMenuFirst,
        DriveMenuSecond,
        EditPath,
        LeaveWaiting,
        NewTab,
        ResetSplit,
        ToggleJobs,
        TogglePreview
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::pane_chrome::bind_keys(cx);
    super::tab_keys::bind_keys(cx);
    crate::side_sheet::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("alt-f1", DriveMenuFirst, Some(PANE)),
        KeyBinding::new("alt-f2", DriveMenuSecond, Some(PANE)),
        KeyBinding::new("ctrl-l", EditPath, Some(PANE)),
        KeyBinding::new("ctrl-t", NewTab, Some(PANE)),
        KeyBinding::new("ctrl-shift-0", ResetSplit, Some(PANE)),
        KeyBinding::new("ctrl-q", TogglePreview, Some(PANE)),
        KeyBinding::new("ctrl-j", ToggleJobs, None),
        KeyBinding::new("escape", LeaveWaiting, Some(WAITING)),
    ]);
}
