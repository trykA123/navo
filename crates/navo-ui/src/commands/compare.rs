use super::Group;
use super::contexts::GLOBAL;

gpui_kit::actions!(
    compare,
    [
        WithOtherPane,
        Refs,
        NextDifference,
        PreviousDifference,
        CopyRight,
        CopyLeft,
        Enter,
        Up,
        Exit,
        Save,
        StopChecking,
        Continue,
        ViewBytes,
        ViewText,
        Stream,
        NextFile,
        PreviousFile,
        ViewMenu,
        Changes,
        Details,
        ToggleViewed,
        Fold,
        OpenFile,
        Search,
        CopyAllLeft,
        CopyAllRight
    ]
);

pub(super) const GROUPS: [Group; 5] = [
    Group {
        context: "FileList",
        commands: &[(
            "compare::WithOtherPane",
            "compare.with_other_pane",
            "cmd-compare-with-other-pane",
        )],
    },
    Group {
        context: GLOBAL,
        commands: &[("compare::Refs", "compare.refs", "cmd-compare-refs")],
    },
    Group {
        context: "Compare",
        commands: &[
            (
                "compare::NextDifference",
                "compare.next_difference",
                "cmd-compare-next-difference",
            ),
            (
                "compare::PreviousDifference",
                "compare.previous_difference",
                "cmd-compare-previous-difference",
            ),
            (
                "compare::CopyRight",
                "compare.copy_right",
                "cmd-compare-copy-right",
            ),
            (
                "compare::CopyLeft",
                "compare.copy_left",
                "cmd-compare-copy-left",
            ),
            ("compare::Enter", "compare.enter", "cmd-compare-enter"),
            ("compare::Up", "compare.up", "cmd-compare-up"),
            ("compare::Exit", "compare.exit", "cmd-compare-exit"),
            (
                "compare::StopChecking",
                "compare.stop_checking",
                "cmd-compare-stop-checking",
            ),
            (
                "compare::Continue",
                "compare.continue",
                "cmd-compare-continue",
            ),
            ("compare::Stream", "compare.stream", "cmd-compare-stream"),
        ],
    },
    Group {
        context: "CompareView",
        commands: &[
            ("compare::Save", "compare.save", "cmd-compare-save"),
            (
                "compare::ViewBytes",
                "compare.view_bytes",
                "cmd-compare-view-bytes",
            ),
            (
                "compare::ViewText",
                "compare.view_text",
                "cmd-compare-view-text",
            ),
            (
                "compare::NextFile",
                "compare.next_file",
                "cmd-compare-next-file",
            ),
            (
                "compare::PreviousFile",
                "compare.previous_file",
                "cmd-compare-previous-file",
            ),
            (
                "compare::ViewMenu",
                "compare.view_menu",
                "cmd-compare-view-menu",
            ),
            ("compare::Changes", "compare.changes", "cmd-compare-changes"),
            ("compare::Details", "compare.details", "cmd-compare-details"),
        ],
    },
    Group {
        context: "CompareStream",
        commands: &[
            (
                "compare::ToggleViewed",
                "compare.toggle_viewed",
                "cmd-compare-toggle-viewed",
            ),
            ("compare::Fold", "compare.fold", "cmd-compare-fold"),
            (
                "compare::OpenFile",
                "compare.open_file",
                "cmd-compare-open-file",
            ),
            ("compare::Search", "compare.search", "cmd-compare-search"),
            (
                "compare::CopyAllLeft",
                "compare.copy_all_left",
                "cmd-compare-copy-all-left",
            ),
            (
                "compare::CopyAllRight",
                "compare.copy_all_right",
                "cmd-compare-copy-all-right",
            ),
        ],
    },
];

/// Commands whose atoms have not landed: out of menus, palette and keymap while Compare is on.
const NOT_BUILT: &[&str] = &[
    "compare.refs",
    "compare.copy_left",
    "compare.copy_right",
    "compare.save",
    "compare.view_bytes",
    "compare.view_text",
    "compare.search",
    "compare.copy_all_left",
    "compare.copy_all_right",
];

/// Whether `name` is a compare command that is not offered: all while Compare is off,
/// and the ones still to be built while it is on.
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with("compare.")
        && (!crate::compare_visible::compare_visible() || NOT_BUILT.contains(&name))
}
