//! The entries of the compare menu (V33): pure lists built from what applies at the cursor line.
//! Each entry runs the same action as its key; the key shown beside it comes from the keymap.

use super::run;
use crate::commands::compare::{
    Changes, CopyLeft, CopyRight, Details, Fold, NextDifference, NextFile, OpenFile, PickToggle,
    PreviousDifference, PreviousFile, RemovePicked, Search, ShowBytesHere, ToggleViewed, ViewMenu,
};
use crate::compare_diff::CopyDirection;
use crate::window_view::viewer::MenuEntry;

/// What a copy at the cursor line would copy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Copying {
    #[default]
    None,
    /// The current change, either way.
    Change,
    /// The picked lines, in the one direction their side allows.
    Picked(CopyDirection),
}

/// What applies at the file diff's cursor line.
#[derive(Debug, Default)]
pub(super) struct FileFacts {
    pub(super) copy: Copying,
    /// Whether picked lines can be removed.
    pub(super) remove: bool,
    /// `Some(picked)` when the cursor line can be picked.
    pub(super) pick: Option<bool>,
    /// Whether the pair has any change to walk to.
    pub(super) changes: bool,
    /// Whether the line under the cursor is over 1 MiB, so it can open in Bytes (V41d).
    pub(super) bytes_here: bool,
}

/// What applies at the Stream's current stop.
#[derive(Debug, Default)]
pub(super) struct StreamFacts {
    /// Whether the stop is on a card.
    pub(super) item: bool,
    /// Whether the stop is on a change that can be copied.
    pub(super) copy: bool,
}

/// Joins the non-empty `groups` with a break between each two.
fn join(groups: Vec<Vec<MenuEntry>>) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !out.is_empty() {
            out.push(MenuEntry::Break);
        }
        out.extend(group);
    }
    out
}

/// Copy entries for `copy`: the change either way, or the picked lines their one way.
fn copy_entries(copy: Copying) -> Vec<MenuEntry> {
    match copy {
        Copying::None => Vec::new(),
        Copying::Change => vec![
            run("compare-menu-copy-left", CopyLeft),
            run("compare-menu-copy-right", CopyRight),
        ],
        Copying::Picked(CopyDirection::ToLeft) => {
            vec![run("compare-menu-copy-picked-left", CopyLeft)]
        }
        Copying::Picked(CopyDirection::ToRight) => {
            vec![run("compare-menu-copy-picked-right", CopyRight)]
        }
    }
}

/// The steps both screens share: next and previous difference and file.
fn walking(differences: bool) -> Vec<MenuEntry> {
    let mut out = Vec::new();
    if differences {
        out.push(run("cmd-compare-next-difference", NextDifference));
        out.push(run("cmd-compare-previous-difference", PreviousDifference));
    }
    out.push(run("cmd-compare-next-file", NextFile));
    out.push(run("cmd-compare-previous-file", PreviousFile));
    out
}

/// The drawers and the View menu both screens share.
fn showing() -> Vec<MenuEntry> {
    vec![
        run("cmd-compare-changes", Changes),
        run("cmd-compare-details", Details),
        run("compare-menu-view", ViewMenu),
    ]
}

/// The entries of the file diff's menu.
pub(super) fn file_entries(facts: &FileFacts) -> Vec<MenuEntry> {
    let mut picking = Vec::new();
    if let Some(picked) = facts.pick {
        let id = if picked {
            "compare-menu-unpick"
        } else {
            "compare-menu-pick"
        };
        picking.push(run(id, PickToggle));
    }
    if facts.remove {
        picking.push(run("cmd-compare-remove-picked", RemovePicked));
    }
    join(vec![
        copy_entries(facts.copy),
        picking,
        if facts.bytes_here {
            vec![run("cmd-compare-show-bytes-here", ShowBytesHere)]
        } else {
            Vec::new()
        },
        walking(facts.changes),
        showing(),
    ])
}

/// The entries of the Stream's menu.
pub(super) fn stream_entries(facts: &StreamFacts) -> Vec<MenuEntry> {
    let card = if facts.item {
        vec![
            run("cmd-compare-open-file", OpenFile),
            run("cmd-compare-toggle-viewed", ToggleViewed),
            run("cmd-compare-fold", Fold),
        ]
    } else {
        Vec::new()
    };
    let copy = if facts.copy {
        Copying::Change
    } else {
        Copying::None
    };
    join(vec![
        copy_entries(copy),
        card,
        walking(facts.item),
        vec![run("cmd-compare-search", Search)],
        showing(),
    ])
}

#[cfg(test)]
mod tests {
    use super::{Copying, FileFacts, StreamFacts, file_entries, stream_entries};
    use crate::commands::{self, compare::is_hidden};
    use crate::compare_diff::CopyDirection;
    use crate::window_view::viewer::MenuEntry;

    /// The action names of the runnable entries.
    fn actions(entries: &[MenuEntry]) -> Vec<&'static str> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Run(_, action, _) => Some(action.name()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn compare_copy_menu_offers_only_what_applies_at_the_cursor() {
        let none = actions(&file_entries(&FileFacts::default()));
        assert!(!none.contains(&"compare::CopyLeft") && !none.contains(&"compare::PickToggle"));
        let change = FileFacts {
            copy: Copying::Change,
            pick: Some(false),
            changes: true,
            ..FileFacts::default()
        };
        let both = actions(&file_entries(&change));
        assert!(both.contains(&"compare::CopyLeft") && both.contains(&"compare::CopyRight"));
        let picked = FileFacts {
            copy: Copying::Picked(CopyDirection::ToRight),
            remove: true,
            ..change
        };
        let one = actions(&file_entries(&picked));
        assert!(one.contains(&"compare::CopyRight") && !one.contains(&"compare::CopyLeft"));
        assert!(one.contains(&"compare::RemovePicked"));
        assert!(
            !none.contains(&"compare::ShowBytesHere"),
            "only a long line offers bytes"
        );
        let long = FileFacts {
            bytes_here: true,
            ..FileFacts::default()
        };
        assert!(actions(&file_entries(&long)).contains(&"compare::ShowBytesHere"));
        let bare = actions(&stream_entries(&StreamFacts::default()));
        assert!(!bare.contains(&"compare::OpenFile") && bare.contains(&"compare::Search"));
    }

    #[test]
    fn compare_copy_menu_never_lists_a_command_that_is_not_built() {
        let _shown = crate::compare_visible::show_compare();
        let file = FileFacts {
            copy: Copying::Change,
            remove: true,
            pick: Some(true),
            changes: true,
            bytes_here: true,
        };
        let stream = StreamFacts {
            item: true,
            copy: true,
        };
        let mut all = file_entries(&file);
        all.extend(stream_entries(&stream));
        for name in actions(&all) {
            let spec = commands::for_action(name).expect("a registered action");
            assert!(!is_hidden(spec.name), "{name}");
        }
    }
}
