use super::Group;

pub(super) const GROUPS: [Group; 10] = [
    Group {
        context: "Tree",
        commands: &[
            (
                "viewer_tree::OpenSubtree",
                "tree.expand_branch",
                "cmd-tree-expand-branch",
            ),
            (
                "viewer_tree::CloseSubtree",
                "tree.collapse_branch",
                "cmd-tree-collapse-branch",
            ),
            (
                "viewer_tree::OpenAll",
                "tree.expand_all",
                "cmd-tree-expand-all",
            ),
            (
                "viewer_tree::CloseAll",
                "tree.collapse_all",
                "cmd-tree-collapse-all",
            ),
            (
                "viewer_tree::ExpandSubtree",
                "tree.expand_branch_or_type",
                "cmd-tree-expand-branch-or-type",
            ),
            (
                "viewer_tree::CollapseAll",
                "tree.collapse_all_or_type",
                "cmd-tree-collapse-all-or-type",
            ),
            (
                "viewer_tree::FoldDepth1",
                "tree.show_depth_one",
                "cmd-tree-show-depth-one",
            ),
            (
                "viewer_tree::FoldDepth2",
                "tree.show_depth_two",
                "cmd-tree-show-depth-two",
            ),
            (
                "viewer_tree::FoldDepth3",
                "tree.show_depth_three",
                "cmd-tree-show-depth-three",
            ),
            (
                "viewer_tree::FoldDepth4",
                "tree.show_depth_four",
                "cmd-tree-show-depth-four",
            ),
            (
                "viewer_tree::FoldDepth5",
                "tree.show_depth_five",
                "cmd-tree-show-depth-five",
            ),
            (
                "viewer_tree::FoldDepth6",
                "tree.show_depth_six",
                "cmd-tree-show-depth-six",
            ),
            (
                "viewer_tree::FoldDepth7",
                "tree.show_depth_seven",
                "cmd-tree-show-depth-seven",
            ),
            (
                "viewer_tree::FoldDepth8",
                "tree.show_depth_eight",
                "cmd-tree-show-depth-eight",
            ),
            (
                "viewer_tree::FoldDepth9",
                "tree.show_depth_nine",
                "cmd-tree-show-depth-nine",
            ),
            (
                "viewer_tree::CopyValue",
                "tree.copy_value",
                "cmd-tree-copy-value",
            ),
            ("viewer_tree::CopyKey", "tree.copy_key", "cmd-tree-copy-key"),
            (
                "viewer_tree::CopyPath",
                "tree.copy_path",
                "cmd-tree-copy-path",
            ),
            (
                "viewer_tree::FilterBack",
                "tree.filter_backspace",
                "cmd-tree-filter-backspace",
            ),
            ("viewer_tree::TypeN", "tree.type_n", "cmd-tree-type-n"),
            ("viewer_tree::TypeP", "tree.type_p", "cmd-tree-type-p"),
            ("viewer_tree::TypeOne", "tree.type_one", "cmd-tree-type-one"),
            (
                "viewer_tree::TypeThree",
                "tree.type_three",
                "cmd-tree-type-three",
            ),
            (
                "viewer_tree::TypeFive",
                "tree.type_five",
                "cmd-tree-type-five",
            ),
        ],
    },
    Group {
        context: "Table",
        commands: &[
            (
                "viewer_table::CellFirst",
                "table.first_cell",
                "cmd-table-first-cell",
            ),
            (
                "viewer_table::CellLast",
                "table.last_cell",
                "cmd-table-last-cell",
            ),
            (
                "viewer_table::CopyCell",
                "table.copy_cell",
                "cmd-table-copy-cell",
            ),
            (
                "viewer_table::CopyRow",
                "table.copy_row",
                "cmd-table-copy-row",
            ),
            (
                "viewer_table::CopyColumn",
                "table.copy_column",
                "cmd-table-copy-column",
            ),
            (
                "viewer_table::SortColumn",
                "table.sort_by_column",
                "cmd-table-sort-by-column",
            ),
        ],
    },
    Group {
        context: "Markdown",
        commands: &[
            (
                "viewer_markdown::OpenLink",
                "markdown.open_link",
                "cmd-markdown-open-link",
            ),
            (
                "viewer_markdown::NextLink",
                "markdown.next_link",
                "cmd-markdown-next-link",
            ),
            (
                "viewer_markdown::PreviousLink",
                "markdown.previous_link",
                "cmd-markdown-previous-link",
            ),
            (
                "viewer_markdown::CopyBlock",
                "markdown.copy_block",
                "cmd-markdown-copy-block",
            ),
            (
                "viewer_markdown::CopyLink",
                "markdown.copy_link",
                "cmd-markdown-copy-link",
            ),
        ],
    },
    Group {
        context: "MemoryImage",
        commands: &[(
            "memory_image::PickItem",
            "memory.go_to_item",
            "cmd-memory-go-to-item",
        )],
    },
    Group {
        context: "Elf",
        commands: &[
            ("elf_view::PickItem", "elf.pick_item", "cmd-elf-pick-item"),
            (
                "elf_view::SortByName",
                "elf.sort_by_name",
                "cmd-elf-sort-by-name",
            ),
            (
                "elf_view::SortBySection",
                "elf.sort_by_section",
                "cmd-elf-sort-by-section",
            ),
            (
                "elf_view::SortByAddress",
                "elf.sort_by_address",
                "cmd-elf-sort-by-address",
            ),
            (
                "elf_view::SortBySize",
                "elf.sort_by_size",
                "cmd-elf-sort-by-size",
            ),
        ],
    },
    Group {
        context: "ElfTable",
        commands: &[
            ("elf_view::TypeN", "elf.type_n", "cmd-elf-type-n"),
            ("elf_view::TypeP", "elf.type_p", "cmd-elf-type-p"),
            ("elf_view::TypeOne", "elf.type_one", "cmd-elf-type-one"),
            (
                "elf_view::TypeThree",
                "elf.type_three",
                "cmd-elf-type-three",
            ),
            ("elf_view::TypeFive", "elf.type_five", "cmd-elf-type-five"),
        ],
    },
    Group {
        context: "Map",
        commands: &[
            ("map_view::PickItem", "map.pick_item", "cmd-map-pick-item"),
            (
                "map_view::NextTable",
                "map.next_table",
                "cmd-map-next-table",
            ),
            (
                "map_view::SortByName",
                "map.sort_by_name",
                "cmd-map-sort-by-name",
            ),
            (
                "map_view::SortBySecond",
                "map.sort_by_second",
                "cmd-map-sort-by-second",
            ),
            (
                "map_view::SortByAddress",
                "map.sort_by_address",
                "cmd-map-sort-by-address",
            ),
            (
                "map_view::SortBySize",
                "map.sort_by_size",
                "cmd-map-sort-by-size",
            ),
        ],
    },
    Group {
        context: "MapTable",
        commands: &[
            ("map_view::TypeN", "map.type_n", "cmd-map-type-n"),
            ("map_view::TypeP", "map.type_p", "cmd-map-type-p"),
            ("map_view::TypeOne", "map.type_one", "cmd-map-type-one"),
            (
                "map_view::TypeThree",
                "map.type_three",
                "cmd-map-type-three",
            ),
            ("map_view::TypeFive", "map.type_five", "cmd-map-type-five"),
        ],
    },
    Group {
        context: "CanTrace",
        commands: &[
            (
                "can_trace::PickId",
                "trace.filter_by_id",
                "cmd-trace-filter-by-id",
            ),
            ("can_trace::SortIds", "trace.sort_ids", "cmd-trace-sort-ids"),
            ("can_trace::TypeOne", "trace.type_one", "cmd-trace-type-one"),
            (
                "can_trace::TypeThree",
                "trace.type_three",
                "cmd-trace-type-three",
            ),
            (
                "can_trace::TypeFive",
                "trace.type_five",
                "cmd-trace-type-five",
            ),
            ("can_trace::TypeN", "trace.type_n", "cmd-trace-type-n"),
            ("can_trace::TypeP", "trace.type_p", "cmd-trace-type-p"),
            (
                "can_trace::FilterBack",
                "trace.filter_backspace",
                "cmd-trace-filter-backspace",
            ),
        ],
    },
    Group {
        context: "Pdf",
        commands: &[
            ("viewer_pdf::FitWidth", "pdf.fit", "cmd-pdf-fit"),
            (
                "viewer_pdf::ActualSize",
                "pdf.actual_size",
                "cmd-pdf-actual-size",
            ),
            ("viewer_pdf::ZoomIn", "pdf.zoom_in", "cmd-pdf-zoom-in"),
            ("viewer_pdf::ZoomOut", "pdf.zoom_out", "cmd-pdf-zoom-out"),
            (
                "viewer_pdf::WholeWord",
                "pdf.whole_word",
                "cmd-pdf-whole-word",
            ),
            ("viewer_pdf::AllHits", "pdf.all_hits", "cmd-pdf-all-hits"),
            ("viewer_pdf::PagesDrawer", "pdf.pages", "cmd-pdf-pages"),
            ("viewer_pdf::CopyText", "pdf.copy", "cmd-pdf-copy"),
            ("viewer_pdf::LinkNext", "pdf.link_next", "cmd-pdf-link-next"),
            (
                "viewer_pdf::LinkPrevious",
                "pdf.link_previous",
                "cmd-pdf-link-previous",
            ),
            (
                "viewer_pdf::LinkFollow",
                "pdf.link_follow",
                "cmd-pdf-link-follow",
            ),
            (
                "viewer_pdf::TrailBack",
                "pdf.trail_back",
                "cmd-pdf-trail-back",
            ),
            (
                "viewer_pdf::TrailForward",
                "pdf.trail_forward",
                "cmd-pdf-trail-forward",
            ),
        ],
    },
];
