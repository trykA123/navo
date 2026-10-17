use crate::prelude::*;

pub(super) const PANE: &str = "FerritePane";
pub(super) const WAITING: &str = "PaneWaiting";

gpui_kit::actions!(
    ferrite_window,
    [
        BrowsePreview,
        DriveMenuFirst,
        DriveMenuSecond,
        EditPath,
        LeaveWaiting,
        NewTab,
        ResetSplit,
        RetryListing,
        ToggleJobs,
        TogglePreview
    ]
);

pub(crate) fn bind_app_keys(cx: &mut App) {
    crate::file_list::bind_keys(cx);
    crate::confirm_dialog::bind_keys(cx);
    crate::prompt_dialog::bind_keys(cx);
    crate::app_menu::bind_keys(cx);
    crate::settings::bind_keys(cx);
    crate::context_menu::bind_keys(cx);
    super::viewer::bind_keys(cx);
    bind_keys(cx);
}

pub fn bind_keys(cx: &mut App) {
    crate::pane_chrome::bind_keys(cx);
    super::tab_keys::bind_keys(cx);
    crate::side_sheet::bind_keys(cx);
    super::preview::bind_keys(cx);
    crate::commands::bind_namespace(cx, "ferrite_window");
}
