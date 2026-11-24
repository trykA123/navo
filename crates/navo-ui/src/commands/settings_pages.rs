//! The Settings page commands: Ctrl+1 to Ctrl+0 jump to a page by its nav digit.

use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "SettingsSheet",
    commands: &[
        (
            "settings_sheet::PageAppearance",
            "settings.page_appearance",
            "cmd-settings-page-appearance",
        ),
        (
            "settings_sheet::PageWindow",
            "settings.page_window",
            "cmd-settings-page-window",
        ),
        (
            "settings_sheet::PageFiles",
            "settings.page_files",
            "cmd-settings-page-files",
        ),
        (
            "settings_sheet::PageCopyDelete",
            "settings.page_copy_delete",
            "cmd-settings-page-copy-delete",
        ),
        (
            "settings_sheet::PageSearch",
            "settings.page_search",
            "cmd-settings-page-search",
        ),
        (
            "settings_sheet::PageViewers",
            "settings.page_viewers",
            "cmd-settings-page-viewers",
        ),
        (
            "settings_sheet::PageTools",
            "settings.page_tools",
            "cmd-settings-page-tools",
        ),
        (
            "settings_sheet::PageTrayClipboard",
            "settings.page_tray_clipboard",
            "cmd-settings-page-tray-clipboard",
        ),
        (
            "settings_sheet::PageKeyboard",
            "settings.page_keyboard",
            "cmd-settings-page-keyboard",
        ),
        (
            "settings_sheet::PageAbout",
            "settings.page_about",
            "cmd-settings-page-about",
        ),
    ],
}];
