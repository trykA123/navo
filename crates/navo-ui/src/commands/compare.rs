use super::Group;
use super::contexts::GLOBAL;

// No handlers yet: U4 adds the Compare contexts and binds this namespace.
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
        ViewBytes,
        ViewText
    ]
);

pub(super) const GROUPS: [Group; 4] = [
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
        ],
    },
];

/// Whether `name` is a compare command, hidden while Compare is not visible.
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with("compare.") && !crate::compare_visible::compare_visible()
}
