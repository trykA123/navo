mod notice;
mod waiting;
mod workspaces;

use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 9] = [
    workspaces::GROUP,
    Group {
        context: GLOBAL,
        commands: &[
            (
                "app_menu::ToggleAppMenu",
                "app_menu.toggle",
                "cmd-app-menu-toggle",
            ),
            (
                "settings_sheet::ToggleSettings",
                "settings.toggle",
                "cmd-settings-toggle",
            ),
            (
                "ferrite_window::ToggleJobs",
                "jobs.toggle",
                "cmd-jobs-toggle",
            ),
            (
                "side_sheet::Narrower",
                "sheet.narrower",
                "cmd-sheet-narrower",
            ),
            ("side_sheet::Wider", "sheet.wider", "cmd-sheet-wider"),
            (
                "command_center::Toggle",
                "app.command_palette",
                "cmd-app-command-palette",
            ),
        ],
    },
    Group {
        context: "NavoPane",
        commands: &[
            ("ferrite_tabs::NextTab", "tab.next", "cmd-tab-next"),
            (
                "ferrite_tabs::PreviousTab",
                "tab.previous",
                "cmd-tab-previous",
            ),
            ("ferrite_tabs::CloseTab", "tab.close", "cmd-tab-close"),
            ("ferrite_tabs::CloneTab", "tab.clone", "cmd-tab-clone"),
            (
                "ferrite_tabs::GoToTab1",
                "tab.go_to_first",
                "cmd-tab-go-to-first",
            ),
            (
                "ferrite_tabs::GoToTab2",
                "tab.go_to_second",
                "cmd-tab-go-to-second",
            ),
            (
                "ferrite_tabs::GoToTab3",
                "tab.go_to_third",
                "cmd-tab-go-to-third",
            ),
            (
                "ferrite_tabs::GoToTab4",
                "tab.go_to_fourth",
                "cmd-tab-go-to-fourth",
            ),
            (
                "ferrite_tabs::GoToTab5",
                "tab.go_to_fifth",
                "cmd-tab-go-to-fifth",
            ),
            (
                "ferrite_tabs::GoToTab6",
                "tab.go_to_sixth",
                "cmd-tab-go-to-sixth",
            ),
            (
                "ferrite_tabs::GoToTab7",
                "tab.go_to_seventh",
                "cmd-tab-go-to-seventh",
            ),
            (
                "ferrite_tabs::GoToTab8",
                "tab.go_to_eighth",
                "cmd-tab-go-to-eighth",
            ),
            (
                "ferrite_tabs::GoToLastTab",
                "tab.go_to_last",
                "cmd-tab-go-to-last",
            ),
            ("ferrite_window::NewTab", "tab.new", "cmd-tab-new"),
            (
                "ferrite_window::DriveMenuFirst",
                "pane.drive_menu_first",
                "cmd-pane-drive-menu-first",
            ),
            (
                "ferrite_window::DriveMenuSecond",
                "pane.drive_menu_second",
                "cmd-pane-drive-menu-second",
            ),
            (
                "ferrite_window::EditPath",
                "pane.edit_path",
                "cmd-pane-edit-path",
            ),
            (
                "ferrite_window::OpenViewMenu",
                "window.view_menu",
                "cmd-window-view-menu",
            ),
            (
                "ferrite_window::ResetSplit",
                "pane.reset_split",
                "cmd-pane-reset-split",
            ),
            (
                "ferrite_window::ShowHiddenTabs",
                "tabs.show_hidden",
                "cmd-tabs-show-hidden",
            ),
            (
                "ferrite_window::ShowHiddenPath",
                "path.show_hidden",
                "cmd-path-show-hidden",
            ),
            (
                "ferrite_window::BrowsePreview",
                "preview.browse",
                "cmd-preview-browse",
            ),
            (
                "ferrite_window::FocusCrashNotice",
                "crash_notice.focus",
                "cmd-crash-notice-focus",
            ),
            (
                "ferrite_window::FocusActivity",
                "activity.focus",
                "cmd-activity-focus",
            ),
            (
                "ferrite_window::OpenTerminal",
                "files.open_terminal",
                "cmd-files-open-terminal",
            ),
            (
                "ferrite_window::ResetColumnWidths",
                "files.reset_column_widths",
                "cmd-files-reset-column-widths",
            ),
        ],
    },
    notice::GROUP,
    waiting::GROUP,
    Group {
        context: "PreviewBrowse",
        commands: &[
            (
                "preview::BrowseUp",
                "preview.cursor_up",
                "cmd-preview-cursor-up",
            ),
            (
                "preview::BrowseDown",
                "preview.cursor_down",
                "cmd-preview-cursor-down",
            ),
            ("preview::BrowseFold", "preview.fold", "cmd-preview-fold"),
            (
                "preview::BrowseUnfold",
                "preview.unfold",
                "cmd-preview-unfold",
            ),
            (
                "preview::BrowseUnfoldAll",
                "preview.unfold_all",
                "cmd-preview-unfold-all",
            ),
            (
                "preview::BrowseFoldAll",
                "preview.fold_all",
                "cmd-preview-fold-all",
            ),
            ("preview::BrowseOpen", "preview.open", "cmd-preview-open"),
            ("preview::BrowseGoTo", "preview.go_to", "cmd-preview-go-to"),
            (
                "preview::BrowseParent",
                "preview.open_parent",
                "cmd-preview-open-parent",
            ),
            (
                "preview::BrowseMark",
                "preview.toggle_mark",
                "cmd-preview-toggle-mark",
            ),
            ("preview::BrowseView", "preview.view", "cmd-preview-view"),
            ("preview::LeaveBrowse", "preview.leave", "cmd-preview-leave"),
            (
                "preview::LeaveAndSwitch",
                "preview.leave_and_switch",
                "cmd-preview-leave-and-switch",
            ),
            ("preview::Recount", "preview.recount", "cmd-preview-recount"),
            (
                "preview::ViewMenu",
                "preview.view_menu",
                "cmd-preview-view-menu",
            ),
            (
                "preview::ToggleTypeBars",
                "preview.type_bars",
                "cmd-preview-type-bars",
            ),
            (
                "preview::ToggleLargestFiles",
                "preview.largest_files",
                "cmd-preview-largest-files",
            ),
        ],
    },
    Group {
        context: "FileList",
        commands: &[
            (
                "command_center::FindContent",
                "command_center.find_content",
                "cmd-command-center-find-content",
            ),
            (
                "file_list::CursorUp",
                "files.cursor_up",
                "cmd-files-cursor-up",
            ),
            (
                "file_list::CursorDown",
                "files.cursor_down",
                "cmd-files-cursor-down",
            ),
            ("file_list::PageUp", "files.page_up", "cmd-files-page-up"),
            (
                "file_list::PageDown",
                "files.page_down",
                "cmd-files-page-down",
            ),
            (
                "file_list::CursorFirst",
                "files.cursor_first",
                "cmd-files-cursor-first",
            ),
            (
                "file_list::CursorLast",
                "files.cursor_last",
                "cmd-files-cursor-last",
            ),
            (
                "file_list::ToggleMark",
                "files.toggle_mark",
                "cmd-files-toggle-mark",
            ),
            (
                "file_list::ToggleMarkDown",
                "files.toggle_mark_down",
                "cmd-files-toggle-mark-down",
            ),
            ("file_list::OpenRow", "files.open", "cmd-files-open"),
            (
                "file_list::OpenParent",
                "files.open_parent",
                "cmd-files-open-parent",
            ),
            ("file_list::SwitchPane", "pane.switch", "cmd-pane-switch"),
            ("file_list::CopyItems", "files.copy", "cmd-files-copy"),
            ("file_list::MoveItems", "files.move", "cmd-files-move"),
            (
                "file_list::ClipboardCopy",
                "files.clipboard_copy",
                "cmd-files-clipboard-copy",
            ),
            (
                "file_list::ClipboardCut",
                "files.clipboard_cut",
                "cmd-files-clipboard-cut",
            ),
            ("file_list::Paste", "files.paste", "cmd-files-paste"),
            (
                "file_list::RecycleItems",
                "files.recycle",
                "cmd-files-recycle",
            ),
            ("file_list::DeleteItems", "files.delete", "cmd-files-delete"),
            ("file_list::GoBack", "pane.go_back", "cmd-pane-go-back"),
            (
                "file_list::GoForward",
                "pane.go_forward",
                "cmd-pane-go-forward",
            ),
            (
                "file_list::ClearFilter",
                "files.clear_filter",
                "cmd-files-clear-filter",
            ),
            ("file_list::RenameItem", "files.rename", "cmd-files-rename"),
            (
                "file_list::MultiRename",
                "files.multi_rename",
                "cmd-files-multi-rename",
            ),
            ("file_list::UndoLast", "files.undo", "cmd-files-undo"),
            (
                "file_list::NewFolder",
                "files.new_folder",
                "cmd-files-new-folder",
            ),
            ("file_list::NewFile", "files.new_file", "cmd-files-new-file"),
            (
                "file_list::MarkPattern",
                "files.mark_pattern",
                "cmd-files-mark-pattern",
            ),
            (
                "file_list::UnmarkPattern",
                "files.unmark_pattern",
                "cmd-files-unmark-pattern",
            ),
            (
                "file_list::InvertMarks",
                "files.invert_marks",
                "cmd-files-invert-marks",
            ),
            (
                "file_list::SortName",
                "files.sort_by_name",
                "cmd-files-sort-by-name",
            ),
            (
                "file_list::SortExt",
                "files.sort_by_extension",
                "cmd-files-sort-by-extension",
            ),
            (
                "file_list::SortDate",
                "files.sort_by_date",
                "cmd-files-sort-by-date",
            ),
            (
                "file_list::SortSize",
                "files.sort_by_size",
                "cmd-files-sort-by-size",
            ),
            ("file_list::EditItem", "files.edit", "cmd-files-edit"),
            (
                "file_list::ChooseEditor",
                "files.edit_with",
                "cmd-files-edit-with",
            ),
            ("file_list::ViewItem", "files.view", "cmd-files-view"),
            ("file_list::Refresh", "files.refresh", "cmd-files-refresh"),
            (
                "file_list::OpenMenu",
                "files.context_menu",
                "cmd-files-context-menu",
            ),
        ],
    },
    Group {
        context: "PathInput",
        commands: &[(
            "path_input::CancelPath",
            "path_input.cancel",
            "cmd-path-input-cancel",
        )],
    },
    Group {
        context: "DriveMenu",
        commands: &[
            ("drive_menu::MenuUp", "drive_menu.up", "cmd-drive-menu-up"),
            (
                "drive_menu::MenuDown",
                "drive_menu.down",
                "cmd-drive-menu-down",
            ),
            (
                "drive_menu::MenuOpen",
                "drive_menu.open",
                "cmd-drive-menu-open",
            ),
            (
                "drive_menu::MenuClose",
                "drive_menu.close",
                "cmd-drive-menu-close",
            ),
        ],
    },
];
