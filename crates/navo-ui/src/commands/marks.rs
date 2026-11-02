use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "FileList",
    commands: &[
        (
            "file_list::ExtendUp",
            "files.extend_up",
            "cmd-files-extend-up",
        ),
        (
            "file_list::ExtendDown",
            "files.extend_down",
            "cmd-files-extend-down",
        ),
        (
            "file_list::ExtendPageUp",
            "files.extend_page_up",
            "cmd-files-extend-page-up",
        ),
        (
            "file_list::ExtendPageDown",
            "files.extend_page_down",
            "cmd-files-extend-page-down",
        ),
        (
            "file_list::ExtendFirst",
            "files.extend_first",
            "cmd-files-extend-first",
        ),
        (
            "file_list::ExtendLast",
            "files.extend_last",
            "cmd-files-extend-last",
        ),
        ("file_list::MarkAll", "files.mark_all", "cmd-files-mark-all"),
        (
            "file_list::UnmarkAll",
            "files.unmark_all",
            "cmd-files-unmark-all",
        ),
        (
            "file_list::RestoreMarks",
            "files.restore_marks",
            "cmd-files-restore-marks",
        ),
        (
            "file_list::MarkExtension",
            "files.mark_extension",
            "cmd-files-mark-extension",
        ),
        (
            "file_list::UnmarkExtension",
            "files.unmark_extension",
            "cmd-files-unmark-extension",
        ),
    ],
}];
