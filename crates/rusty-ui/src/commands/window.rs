use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 8] = [
    Group {
        context: GLOBAL,
        commands: &[
            ("app_menu::ToggleAppMenu", "app_menu.toggle", "App menu"),
            (
                "settings_sheet::ToggleSettings",
                "settings.toggle",
                "All settings",
            ),
            (
                "ferrite_window::ToggleJobs",
                "jobs.toggle",
                "Show or hide jobs",
            ),
            (
                "side_sheet::Narrower",
                "sheet.narrower",
                "Narrower side sheet",
            ),
            ("side_sheet::Wider", "sheet.wider", "Wider side sheet"),
        ],
    },
    Group {
        context: "FerritePane",
        commands: &[
            ("ferrite_tabs::NextTab", "tab.next", "Next tab"),
            ("ferrite_tabs::PreviousTab", "tab.previous", "Previous tab"),
            ("ferrite_tabs::CloseTab", "tab.close", "Close tab"),
            ("ferrite_tabs::GoToTab1", "tab.go_to_first", "Go to tab 1"),
            ("ferrite_tabs::GoToTab2", "tab.go_to_second", "Go to tab 2"),
            ("ferrite_tabs::GoToTab3", "tab.go_to_third", "Go to tab 3"),
            ("ferrite_tabs::GoToTab4", "tab.go_to_fourth", "Go to tab 4"),
            ("ferrite_tabs::GoToTab5", "tab.go_to_fifth", "Go to tab 5"),
            ("ferrite_tabs::GoToTab6", "tab.go_to_sixth", "Go to tab 6"),
            ("ferrite_tabs::GoToTab7", "tab.go_to_seventh", "Go to tab 7"),
            ("ferrite_tabs::GoToTab8", "tab.go_to_eighth", "Go to tab 8"),
            (
                "ferrite_tabs::GoToLastTab",
                "tab.go_to_last",
                "Go to last tab",
            ),
            ("ferrite_window::NewTab", "tab.new", "New tab"),
            (
                "ferrite_window::DriveMenuFirst",
                "pane.drive_menu_first",
                "Drives of pane 1",
            ),
            (
                "ferrite_window::DriveMenuSecond",
                "pane.drive_menu_second",
                "Drives of pane 2",
            ),
            ("ferrite_window::EditPath", "pane.edit_path", "Edit path"),
            (
                "ferrite_window::OpenViewMenu",
                "window.view_menu",
                "Window view menu",
            ),
            (
                "ferrite_window::ResetSplit",
                "pane.reset_split",
                "Reset pane split",
            ),
            (
                "ferrite_window::BrowsePreview",
                "preview.browse",
                "Browse in preview",
            ),
            (
                "ferrite_window::FocusCrashNotice",
                "crash_notice.focus",
                "Go to the crash notice",
            ),
        ],
    },
    Group {
        context: "CrashNotice",
        commands: &[
            (
                "ferrite_window::SaveCrashReport",
                "crash_notice.save",
                "Save crash report",
            ),
            (
                "ferrite_window::DismissCrashNotice",
                "crash_notice.dismiss",
                "Dismiss crash notice",
            ),
        ],
    },
    Group {
        context: "PaneWaiting",
        commands: &[
            (
                "ferrite_window::LeaveWaiting",
                "pane.leave_waiting",
                "Stop waiting",
            ),
            (
                "ferrite_window::RetryListing",
                "pane.retry_listing",
                "Retry listing",
            ),
        ],
    },
    Group {
        context: "PreviewBrowse",
        commands: &[
            ("preview::BrowseUp", "preview.cursor_up", "Up"),
            ("preview::BrowseDown", "preview.cursor_down", "Down"),
            ("preview::BrowseFold", "preview.fold", "Fold folder"),
            ("preview::BrowseUnfold", "preview.unfold", "Unfold folder"),
            (
                "preview::BrowseUnfoldAll",
                "preview.unfold_all",
                "Unfold everything below",
            ),
            (
                "preview::BrowseFoldAll",
                "preview.fold_all",
                "Fold everything below",
            ),
            ("preview::BrowseOpen", "preview.open", "Open in preview"),
            ("preview::BrowseGoTo", "preview.go_to", "Open in pane"),
            (
                "preview::BrowseParent",
                "preview.open_parent",
                "Up one folder",
            ),
            (
                "preview::BrowseMark",
                "preview.toggle_mark",
                "Mark and count",
            ),
            ("preview::BrowseView", "preview.view", "View file"),
            ("preview::LeaveBrowse", "preview.leave", "Close preview"),
            (
                "preview::LeaveAndSwitch",
                "preview.leave_and_switch",
                "Leave preview, switch pane",
            ),
            ("preview::Recount", "preview.recount", "Recount folder"),
            (
                "preview::ViewMenu",
                "preview.view_menu",
                "Preview view menu",
            ),
            (
                "preview::ToggleTypeBars",
                "preview.type_bars",
                "Show or hide type bars",
            ),
            (
                "preview::ToggleLargestFiles",
                "preview.largest_files",
                "Show or hide the largest files",
            ),
        ],
    },
    Group {
        context: "FileList",
        commands: &[
            ("file_list::CursorUp", "files.cursor_up", "Up"),
            ("file_list::CursorDown", "files.cursor_down", "Down"),
            ("file_list::PageUp", "files.page_up", "Page up"),
            ("file_list::PageDown", "files.page_down", "Page down"),
            ("file_list::CursorFirst", "files.cursor_first", "First row"),
            ("file_list::CursorLast", "files.cursor_last", "Last row"),
            ("file_list::ToggleMark", "files.toggle_mark", "Mark"),
            (
                "file_list::ToggleMarkDown",
                "files.toggle_mark_down",
                "Mark and move down",
            ),
            ("file_list::OpenRow", "files.open", "Open"),
            (
                "file_list::OpenParent",
                "files.open_parent",
                "Open parent folder",
            ),
            ("file_list::SwitchPane", "pane.switch", "Switch pane"),
            ("file_list::CopyItems", "files.copy", "Copy"),
            ("file_list::MoveItems", "files.move", "Move"),
            (
                "file_list::ClipboardCopy",
                "files.clipboard_copy",
                "Copy to clipboard",
            ),
            (
                "file_list::ClipboardCut",
                "files.clipboard_cut",
                "Cut to clipboard",
            ),
            ("file_list::Paste", "files.paste", "Paste"),
            (
                "file_list::RecycleItems",
                "files.recycle",
                "Move to Recycle Bin",
            ),
            (
                "file_list::DeleteItems",
                "files.delete",
                "Delete permanently",
            ),
            ("file_list::GoBack", "pane.go_back", "Back"),
            ("file_list::GoForward", "pane.go_forward", "Forward"),
            (
                "file_list::ClearFilter",
                "files.clear_filter",
                "Clear filter",
            ),
            ("file_list::RenameItem", "files.rename", "Rename"),
            ("file_list::NewFolder", "files.new_folder", "New folder"),
            ("file_list::NewFile", "files.new_file", "New file"),
            (
                "file_list::MarkPattern",
                "files.mark_pattern",
                "Mark by pattern",
            ),
            (
                "file_list::UnmarkPattern",
                "files.unmark_pattern",
                "Unmark by pattern",
            ),
            (
                "file_list::InvertMarks",
                "files.invert_marks",
                "Invert marks",
            ),
            ("file_list::SortName", "files.sort_by_name", "Sort by name"),
            (
                "file_list::SortExt",
                "files.sort_by_extension",
                "Sort by extension",
            ),
            ("file_list::SortDate", "files.sort_by_date", "Sort by date"),
            ("file_list::SortSize", "files.sort_by_size", "Sort by size"),
            ("file_list::EditItem", "files.edit", "Edit"),
            ("file_list::ChooseEditor", "files.edit_with", "Edit with"),
            ("file_list::ViewItem", "files.view", "View"),
            ("file_list::Refresh", "files.refresh", "Refresh"),
            ("file_list::OpenMenu", "files.context_menu", "Context menu"),
        ],
    },
    Group {
        context: "PathInput",
        commands: &[(
            "path_input::CancelPath",
            "path_input.cancel",
            "Cancel path edit",
        )],
    },
    Group {
        context: "DriveMenu",
        commands: &[
            ("drive_menu::MenuUp", "drive_menu.up", "Up"),
            ("drive_menu::MenuDown", "drive_menu.down", "Down"),
            ("drive_menu::MenuOpen", "drive_menu.open", "Open drive"),
            (
                "drive_menu::MenuClose",
                "drive_menu.close",
                "Close drive menu",
            ),
        ],
    },
];
