use super::Group;

pub(super) const GROUPS: [Group; 3] = [
    Group {
        context: "Tree",
        commands: &[
            (
                "viewer_tree::OpenSubtree",
                "tree.expand_branch",
                "Open branch",
            ),
            (
                "viewer_tree::CloseSubtree",
                "tree.collapse_branch",
                "Fold branch",
            ),
            ("viewer_tree::OpenAll", "tree.expand_all", "Open all"),
            ("viewer_tree::CloseAll", "tree.collapse_all", "Fold all"),
            (
                "viewer_tree::ExpandSubtree",
                "tree.expand_branch_or_type",
                "Open branch or type *",
            ),
            (
                "viewer_tree::CollapseAll",
                "tree.collapse_all_or_type",
                "Fold all or type -",
            ),
            (
                "viewer_tree::FoldDepth1",
                "tree.show_depth_one",
                "Show depth 1",
            ),
            (
                "viewer_tree::FoldDepth2",
                "tree.show_depth_two",
                "Show depth 2",
            ),
            (
                "viewer_tree::FoldDepth3",
                "tree.show_depth_three",
                "Show depth 3",
            ),
            (
                "viewer_tree::FoldDepth4",
                "tree.show_depth_four",
                "Show depth 4",
            ),
            (
                "viewer_tree::FoldDepth5",
                "tree.show_depth_five",
                "Show depth 5",
            ),
            (
                "viewer_tree::FoldDepth6",
                "tree.show_depth_six",
                "Show depth 6",
            ),
            (
                "viewer_tree::FoldDepth7",
                "tree.show_depth_seven",
                "Show depth 7",
            ),
            (
                "viewer_tree::FoldDepth8",
                "tree.show_depth_eight",
                "Show depth 8",
            ),
            (
                "viewer_tree::FoldDepth9",
                "tree.show_depth_nine",
                "Show depth 9",
            ),
            ("viewer_tree::CopyValue", "tree.copy_value", "Copy value"),
            ("viewer_tree::CopyKey", "tree.copy_key", "Copy key"),
            ("viewer_tree::CopyPath", "tree.copy_path", "Copy path"),
            (
                "viewer_tree::FilterBack",
                "tree.filter_backspace",
                "Delete last filter character",
            ),
            (
                "viewer_tree::TypeN",
                "tree.type_n",
                "Type n while filtering",
            ),
            (
                "viewer_tree::TypeP",
                "tree.type_p",
                "Type p while filtering",
            ),
            (
                "viewer_tree::TypeOne",
                "tree.type_one",
                "Type 1 while filtering",
            ),
            (
                "viewer_tree::TypeThree",
                "tree.type_three",
                "Type 3 while filtering",
            ),
            (
                "viewer_tree::TypeFive",
                "tree.type_five",
                "Type 5 while filtering",
            ),
        ],
    },
    Group {
        context: "Table",
        commands: &[
            (
                "viewer_table::CellFirst",
                "table.first_cell",
                "First cell in row",
            ),
            (
                "viewer_table::CellLast",
                "table.last_cell",
                "Last cell in row",
            ),
            ("viewer_table::CopyCell", "table.copy_cell", "Copy cell"),
            ("viewer_table::CopyRow", "table.copy_row", "Copy row"),
            (
                "viewer_table::CopyColumn",
                "table.copy_column",
                "Copy column",
            ),
            (
                "viewer_table::SortColumn",
                "table.sort_by_column",
                "Sort by this column",
            ),
        ],
    },
    Group {
        context: "Markdown",
        commands: &[
            (
                "viewer_markdown::OpenLink",
                "markdown.open_link",
                "Open link",
            ),
            (
                "viewer_markdown::NextLink",
                "markdown.next_link",
                "Next link",
            ),
            (
                "viewer_markdown::PreviousLink",
                "markdown.previous_link",
                "Previous link",
            ),
            (
                "viewer_markdown::CopyBlock",
                "markdown.copy_block",
                "Copy block",
            ),
            (
                "viewer_markdown::CopyLink",
                "markdown.copy_link",
                "Copy link",
            ),
        ],
    },
];
