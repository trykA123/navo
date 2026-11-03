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
            (
                "compare::Continue",
                "compare.continue",
                "cmd-compare-continue",
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

/// Commands whose atoms have not landed: out of menus, palette and keymap while Compare is on.
const NOT_BUILT: &[&str] = &[
    "compare.refs",
    "compare.copy_left",
    "compare.copy_right",
    "compare.save",
    "compare.view_bytes",
    "compare.view_text",
];

/// Whether `name` is a compare command that is not offered: all while Compare is off,
/// and the ones still to be built while it is on.
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with("compare.")
        && (!crate::compare_visible::compare_visible() || NOT_BUILT.contains(&name))
}
