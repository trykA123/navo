use super::Group;

pub(super) const GROUPS: [Group; 5] = [
    Group {
        context: "Viewer",
        commands: &[
            ("viewer::SaveFile", "viewer.save", "cmd-viewer-save"),
            ("viewer::CloseViewer", "viewer.close", "cmd-viewer-close"),
            ("viewer::EditFile", "viewer.edit", "cmd-viewer-edit"),
            (
                "viewer::ShowText",
                "viewer.show_text",
                "cmd-viewer-show-text",
            ),
            ("viewer::ShowHex", "viewer.show_hex", "cmd-viewer-show-hex"),
            (
                "viewer::ShowRich",
                "viewer.show_rich",
                "cmd-viewer-show-rich",
            ),
            (
                "viewer::ListerText",
                "lister.show_text",
                "cmd-lister-show-text",
            ),
            (
                "viewer::ListerHex",
                "lister.show_hex",
                "cmd-lister-show-hex",
            ),
            (
                "viewer::ListerRich",
                "lister.show_rich",
                "cmd-lister-show-rich",
            ),
            (
                "viewer::FindOnEnter",
                "lister.find_next",
                "cmd-lister-find-next",
            ),
            (
                "viewer::SpaceKey",
                "lister.page_or_next_image",
                "cmd-lister-page-or-next-image",
            ),
            (
                "viewer::BackspaceKey",
                "lister.previous_image",
                "cmd-lister-previous-image",
            ),
            (
                "viewer::NextFile",
                "viewer.next_file",
                "cmd-viewer-next-file",
            ),
            (
                "viewer::PreviousFile",
                "viewer.previous_file",
                "cmd-viewer-previous-file",
            ),
            ("viewer::RowUp", "viewer.line_up", "cmd-viewer-line-up"),
            (
                "viewer::RowDown",
                "viewer.line_down",
                "cmd-viewer-line-down",
            ),
            ("viewer::PageUp", "viewer.page_up", "cmd-viewer-page-up"),
            (
                "viewer::PageDown",
                "viewer.page_down",
                "cmd-viewer-page-down",
            ),
            (
                "viewer::GoStart",
                "viewer.go_to_start",
                "cmd-viewer-go-to-start",
            ),
            ("viewer::GoEnd", "viewer.go_to_end", "cmd-viewer-go-to-end"),
            (
                "viewer::ScrollLeft",
                "viewer.scroll_left",
                "cmd-viewer-scroll-left",
            ),
            (
                "viewer::ScrollRight",
                "viewer.scroll_right",
                "cmd-viewer-scroll-right",
            ),
            (
                "viewer::OpenGoTo",
                "viewer.go_to_line",
                "cmd-viewer-go-to-line",
            ),
            ("viewer::Back", "viewer.back", "cmd-viewer-back"),
            (
                "viewer::ShowLineEnds",
                "viewer.show_line_ends",
                "cmd-viewer-show-line-ends",
            ),
            ("viewer::ToggleFollow", "viewer.follow", "cmd-viewer-follow"),
            (
                "viewer::ShowMenu",
                "viewer.context_menu",
                "cmd-viewer-context-menu",
            ),
            (
                "viewer::ViewMenu",
                "viewer.view_menu",
                "cmd-viewer-view-menu",
            ),
            (
                "viewer::ChooseEncoding",
                "viewer.encoding",
                "cmd-viewer-encoding",
            ),
            (
                "viewer::LeftDrawer",
                "view.left_drawer",
                "cmd-view-left-drawer",
            ),
            (
                "viewer::RightDrawer",
                "view.right_drawer",
                "cmd-view-right-drawer",
            ),
            (
                "viewer::ToggleLineNumbers",
                "view.line_numbers",
                "cmd-view-line-numbers",
            ),
            (
                "viewer::ToggleOpensInspector",
                "view.opens_inspector",
                "cmd-view-opens-inspector",
            ),
            (
                "viewer::ToggleMemoryStrip",
                "view.memory_strip",
                "cmd-view-memory-strip",
            ),
            ("viewer::ToggleCharts", "view.charts", "cmd-view-charts"),
            (
                "viewer::ToggleRowBars",
                "view.row_bars",
                "cmd-view-row-bars",
            ),
            ("viewer::ViewSimple", "view.simple", "cmd-view-simple"),
            ("viewer::ViewRich", "view.rich", "cmd-view-rich"),
            ("viewer::ViewCustom", "view.custom", "cmd-view-custom"),
            (
                "viewer::KeepViewDefault",
                "view.keep_as_default",
                "cmd-view-keep-as-default",
            ),
            (
                "viewer::ResetView",
                "view.reset_to_defaults",
                "cmd-view-reset-to-defaults",
            ),
            (
                "viewer::CopyPathLine",
                "viewer.copy_path_line",
                "cmd-viewer-copy-path-line",
            ),
            ("viewer::OpenFind", "find.open", "cmd-find-open"),
            ("viewer::FindNext", "find.next", "cmd-find-next"),
            ("viewer::FindPrevious", "find.previous", "cmd-find-previous"),
            (
                "viewer::FindSelection",
                "find.selection",
                "cmd-find-selection",
            ),
            (
                "viewer::MatchCase",
                "find.match_case",
                "cmd-find-match-case",
            ),
            ("viewer::Mark1", "mark.colour_one", "cmd-mark-colour-one"),
            ("viewer::Mark2", "mark.colour_two", "cmd-mark-colour-two"),
            (
                "viewer::Mark3",
                "mark.colour_three",
                "cmd-mark-colour-three",
            ),
            ("viewer::Mark4", "mark.colour_four", "cmd-mark-colour-four"),
            ("viewer::Mark5", "mark.colour_five", "cmd-mark-colour-five"),
            ("viewer::Mark6", "mark.colour_six", "cmd-mark-colour-six"),
            (
                "viewer::Mark7",
                "mark.colour_seven",
                "cmd-mark-colour-seven",
            ),
            (
                "viewer::Mark8",
                "mark.colour_eight",
                "cmd-mark-colour-eight",
            ),
            ("viewer::ClearMarks", "mark.clear", "cmd-mark-clear"),
            (
                "viewer::ToggleBookmark",
                "bookmark.toggle",
                "cmd-bookmark-toggle",
            ),
            ("viewer::NextBookmark", "bookmark.next", "cmd-bookmark-next"),
            (
                "viewer::PreviousBookmark",
                "bookmark.previous",
                "cmd-bookmark-previous",
            ),
            (
                "viewer::TogglePreview",
                "editor.toggle_preview",
                "cmd-editor-toggle-preview",
            ),
            (
                "viewer::FocusPreview",
                "editor.focus_preview",
                "cmd-editor-focus-preview",
            ),
        ],
    },
    Group {
        context: "MarkdownPreview",
        commands: &[(
            "viewer::LeavePreview",
            "editor.leave_preview",
            "cmd-editor-leave-preview",
        )],
    },
    Group {
        context: "ViewerFind",
        commands: &[
            (
                "viewer::OlderSearch",
                "find.older_search",
                "cmd-find-older-search",
            ),
            (
                "viewer::NewerSearch",
                "find.newer_search",
                "cmd-find-newer-search",
            ),
            ("viewer::FindAll", "find.all", "cmd-find-all"),
        ],
    },
    Group {
        context: "FindAll",
        commands: &[
            ("viewer::ListUp", "find.result_up", "cmd-find-result-up"),
            (
                "viewer::ListDown",
                "find.result_down",
                "cmd-find-result-down",
            ),
            (
                "viewer::ListGo",
                "find.go_to_result",
                "cmd-find-go-to-result",
            ),
            (
                "viewer::ListClose",
                "find.close_results",
                "cmd-find-close-results",
            ),
        ],
    },
    Group {
        context: "Image",
        commands: &[
            ("viewer::FitImage", "image.fit", "cmd-image-fit"),
            (
                "viewer::ActualSize",
                "image.actual_size",
                "cmd-image-actual-size",
            ),
            ("viewer::ZoomIn", "image.zoom_in", "cmd-image-zoom-in"),
            ("viewer::ZoomOut", "image.zoom_out", "cmd-image-zoom-out"),
            (
                "viewer::ToggleInfo",
                "image.toggle_info",
                "cmd-image-toggle-info",
            ),
            ("viewer::CopyImage", "image.copy", "cmd-image-copy"),
            (
                "viewer::CopyFilePath",
                "image.copy_path",
                "cmd-image-copy-path",
            ),
        ],
    },
];
