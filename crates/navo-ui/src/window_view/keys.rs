use gpui_kit::KeyBinding;

use crate::commands::{Keymap, for_action, set_active};
use crate::prelude::*;

pub(super) const PANE: &str = "NavoPane";
pub(super) const WAITING: &str = "PaneWaiting";
pub(super) const CRASH: &str = "CrashNotice";

gpui_kit::actions!(
    navo_window,
    [
        BrowsePreview,
        ButtonBarContextMenu,
        ButtonBarEditorAdd,
        ButtonBarEditorDone,
        ButtonBarEditorDuplicate,
        ButtonBarEditorMoveLeft,
        ButtonBarEditorMoveRight,
        ButtonBarEditorRemove,
        ButtonBarEditorUndo,
        ButtonBarLeave,
        ButtonBarNext,
        ButtonBarOpenMenu,
        ButtonBarPrevious,
        ButtonBarRun,
        ClearClipboardHistory,
        DeleteClipboardData,
        DismissCrashNotice,
        DriveMenuFirst,
        DriveMenuSecond,
        EditButtonBar,
        EditPath,
        EditUserCommands,
        FocusActivity,
        FocusButtonBar,
        FocusCrashNotice,
        FocusFindResults,
        LeaveWaiting,
        NewTab,
        OpenClipboard,
        OpenKeyMap,
        OpenRecentlyDeleted,
        OpenRecoveryFolder,
        OpenTerminal,
        OpenUserGuide,
        OpenViewMenu,
        PauseClipboard,
        RecordPerformance,
        ResetColumnWidths,
        ResetSplit,
        RetryListing,
        SaveCrashReport,
        ShowHiddenPath,
        ShowHiddenTabs,
        StopRecording,
        ToggleJobs,
        ToggleTerminal
    ]
);

pub(crate) fn rebind_app_keys(keymap: Keymap, cx: &mut App) {
    let others: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|binding| for_action(binding.action().name()).is_none())
        .cloned()
        .collect();
    set_active(cx, keymap);
    cx.clear_key_bindings();
    cx.bind_keys(others);
    bind_app_keys(cx);
}

pub(crate) fn bind_app_keys(cx: &mut App) {
    crate::file_list::bind_keys(cx);
    crate::commands::kit_keys::bind_confirm_dialog(cx);
    crate::prompt_dialog::bind_keys(cx);
    crate::multi_rename::bind_keys(cx);
    crate::app_menu::bind_keys(cx);
    crate::settings::bind_keys(cx);
    crate::shortcuts::bind_keys(cx);
    crate::commands::kit_keys::bind_context_menu(cx);
    super::viewer::bind_keys(cx);
    bind_keys(cx);
    crate::commands::bind_user_keys(cx);
    crate::commands::bind_user_command_keys(cx);
}

pub fn bind_keys(cx: &mut App) {
    crate::pane_chrome::bind_keys(cx);
    super::tab_keys::bind_keys(cx);
    crate::side_sheet::bind_keys(cx);
    crate::command_center::bind_keys(cx);
    crate::clip_palette::bind_keys(cx);
    crate::activity_strip::bind_keys(cx);
    super::preview::bind_keys(cx);
    super::compare_keys::bind_keys(cx);
    crate::commands::bind_namespace(cx, "navo_window");
    crate::commands::bind_namespace(cx, "find_results");
}
