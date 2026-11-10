//! Guard over the built-in keymap: no key does two things on one screen node.

use super::{Bind, Keymap, normal_key};
use crate::commands::GLOBAL;
use crate::commands::bind::predicate_of;
use crate::commands::focus_paths::{Reach, knows, screen_reach};

/// Outer commands that a focused part is meant to take over.
const YIELDS: [&str; 12] = [
    "viewer.line_up",
    "viewer.line_down",
    "viewer.scroll_left",
    "viewer.scroll_right",
    "lister.find_next",
    "lister.previous_image",
    "find.selection",
    "viewer.copy_path_line",
    "editor.focus_preview",
    "settings.row_down",
    "settings.edit_value",
    "settings.close",
];

/// Contexts that type into a filter, so they may shadow single characters and Backspace.
const TYPING: [&str; 4] = ["Tree", "ElfTable", "MapTable", "CanTrace"];

/// Screen roots that the focus model does not name because they nest nothing.
const STANDALONE: [&str; 17] = [
    "ActivityDrawer",
    "ActivityStrip",
    "AppMenu",
    "ClipPalette",
    "CommandCenter",
    "ConfirmDialog",
    "ContextMenu",
    "CrashNotice",
    "LayoutMode",
    "DriveMenu",
    "PathInput",
    "PopupMenu",
    "PromptDialog",
    "RecentlyDeleted",
    "SettingsSheet",
    "Shortcuts",
    "WorkspaceTray",
];

/// `Nest` allows every pair whose inner context is the given one or sits inside it.
/// `Pair` allows two named commands on one key. `Wins` lets one command take a key from
/// whatever sits outside it.
enum Allow {
    Wins {
        key: &'static str,
        command: &'static str,
        context: &'static str,
    },
    Nest {
        outer: &'static str,
        inner: &'static str,
    },
    Pair {
        key: &'static str,
        first: (&'static str, &'static str),
        second: (&'static str, &'static str),
    },
}

const ALLOW: [Allow; 8] = [
    // Esc leaves the Markdown preview before the viewer closes.
    Allow::Wins {
        key: "escape",
        command: "editor.leave_preview",
        context: "MarkdownPreview",
    },
    // Enter in the open Stream outline jumps to the item instead of opening the card's file.
    Allow::Wins {
        key: "enter",
        command: "find.go_to_result",
        context: "CompareOutline",
    },
    // Esc closes the find results before the viewer closes.
    Allow::Wins {
        key: "escape",
        command: "find.close_results",
        context: "FindAll",
    },
    // Ctrl+N inside the clipboard palette makes a new item, like Ctrl+K opens its actions.
    Allow::Pair {
        key: "ctrl-n",
        first: ("window.new", GLOBAL),
        second: ("clip_palette.new_item", "ClipPalette"),
    },
    Allow::Nest {
        outer: "PreviewBrowse",
        inner: "Viewer",
    },
    // The compare node sits inside the list and takes Enter, Backspace and Esc from it.
    Allow::Nest {
        outer: "FileList",
        inner: "Compare",
    },
    Allow::Pair {
        key: "ctrl-k",
        first: ("app.command_palette", GLOBAL),
        second: ("clip_palette.open_actions", "ClipPalette"),
    },
    Allow::Pair {
        key: "ctrl-shift-v",
        first: ("multi_rename.paste_names", "MultiRename"),
        second: ("clipboard.open", "!Viewer"),
    },
];

/// A real conflict of today. Command names match by prefix. Later atoms delete their entries.
struct Known {
    outer: (&'static str, &'static str),
    inner: (&'static str, &'static [&'static str]),
}

const KNOWN: [Known; 0] = [];

type Side = (String, String);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Conflict {
    key: String,
    first: Side,
    second: Side,
    reach: Reach,
}

struct Lists {
    yields: Vec<bool>,
    typing: Vec<bool>,
    allow: Vec<bool>,
    known: Vec<bool>,
    standalone: Vec<bool>,
}

impl Lists {
    fn new() -> Self {
        Self {
            yields: vec![false; YIELDS.len()],
            typing: vec![false; TYPING.len()],
            allow: vec![false; ALLOW.len()],
            known: vec![false; KNOWN.len()],
            standalone: vec![false; STANDALONE.len()],
        }
    }

    fn unused(&self) -> Vec<String> {
        let rows = [
            ("YIELDS", &self.yields),
            ("TYPING", &self.typing),
            ("ALLOW", &self.allow),
            ("KNOWN", &self.known),
            ("STANDALONE", &self.standalone),
        ];
        rows.iter()
            .flat_map(|(name, used)| {
                let unused = used.iter().enumerate().filter(|(_, used)| !**used);
                unused.map(move |(at, _)| format!("{name} entry {at} is never used"))
            })
            .collect()
    }
}

fn base(context: &str) -> &str {
    context.split(" > ").next().unwrap_or(context)
}

/// Reach of two registered contexts. `!Viewer` is live everywhere except inside a viewer.
fn reach(mine: &str, theirs: &str) -> Option<Reach> {
    match (mine, theirs) {
        ("!Viewer", "!Viewer") => Some(Reach::Same),
        ("!Viewer", other) | (other, "!Viewer") => {
            let inside = matches!(
                screen_reach(other, "Viewer"),
                Some(Reach::Same | Reach::Outer)
            );
            (!inside).then_some(Reach::Same)
        }
        _ => screen_reach(mine, theirs),
    }
}

fn has_key(bind: &Bind, key: &str) -> bool {
    bind.keys
        .iter()
        .any(|bound| normal_key(bound).as_deref() == Some(key))
}

fn clashes(keymap: &Keymap) -> Vec<Conflict> {
    let binds = keymap.binds();
    let mut found = Vec::new();
    for (at, first) in binds.iter().enumerate() {
        for second in &binds[at + 1..] {
            if first.spec.name == second.spec.name {
                continue;
            }
            let (one, two) = (predicate_of(first), predicate_of(second));
            let Some(reach) = reach(one, two) else {
                continue;
            };
            let keys = first.keys.iter().filter_map(|key| normal_key(key));
            for key in keys.filter(|key| has_key(second, key)) {
                found.push(Conflict {
                    key,
                    first: (first.spec.name.to_owned(), one.to_owned()),
                    second: (second.spec.name.to_owned(), two.to_owned()),
                    reach,
                });
            }
        }
    }
    found
}

fn is_inside(context: &str, home: &str) -> bool {
    matches!(
        screen_reach(base(context), home),
        Some(Reach::Same | Reach::Outer)
    )
}

/// The two sides as (outer, inner) when one context sits inside the other.
fn ordered(conflict: &Conflict) -> Option<(&Side, &Side)> {
    match conflict.reach {
        Reach::Outer => Some((&conflict.second, &conflict.first)),
        Reach::Inner => Some((&conflict.first, &conflict.second)),
        Reach::Same => None,
    }
}

fn is_typed(key: &str) -> bool {
    key.chars().count() == 1 || key == "backspace"
}

fn matches_known(known: &Known, outer: &Side, inner: &Side) -> bool {
    outer.0.starts_with(known.outer.0)
        && outer.1 == known.outer.1
        && inner.0.starts_with(known.inner.0)
        && known.inner.1.contains(&inner.1.as_str())
}

fn matches_allow(allow: &Allow, conflict: &Conflict) -> bool {
    match allow {
        Allow::Nest { outer, inner } => ordered(conflict)
            .is_some_and(|(top, low)| base(&top.1) == *outer && is_inside(&low.1, inner)),
        Allow::Wins {
            key,
            command,
            context,
        } => {
            conflict.key == *key
                && ordered(conflict)
                    .is_some_and(|(_, low)| low.0 == *command && base(&low.1) == *context)
        }
        Allow::Pair { key, first, second } => {
            let named = |wanted: &(&str, &str)| {
                [&conflict.first, &conflict.second]
                    .iter()
                    .any(|side| side.0 == wanted.0 && base(&side.1) == base(wanted.1))
            };
            conflict.key == *key && named(first) && named(second)
        }
    }
}

fn mark(used: &mut [bool], at: usize, excused: &mut bool) {
    used[at] = true;
    *excused = true;
}

/// Marks every list entry that excuses the conflict. True when something does.
fn excuse(conflict: &Conflict, lists: &mut Lists) -> bool {
    let mut excused = false;
    for (at, allow) in ALLOW.iter().enumerate() {
        if matches_allow(allow, conflict) {
            mark(&mut lists.allow, at, &mut excused);
        }
    }
    let sides = ordered(conflict);
    let pairs = sides.map_or_else(
        || {
            vec![
                (&conflict.first, &conflict.second),
                (&conflict.second, &conflict.first),
            ]
        },
        |pair| vec![pair],
    );
    for (at, known) in KNOWN.iter().enumerate() {
        if pairs.iter().any(|(o, i)| matches_known(known, o, i)) {
            mark(&mut lists.known, at, &mut excused);
        }
    }
    // Yielding and typing apply only between nested contexts, never on one node.
    if let Some((outer, inner)) = sides {
        for (at, name) in YIELDS.iter().enumerate() {
            if outer.0 == *name {
                mark(&mut lists.yields, at, &mut excused);
            }
        }
        for (at, context) in TYPING.iter().enumerate() {
            if base(&inner.1) == *context && is_typed(&conflict.key) {
                mark(&mut lists.typing, at, &mut excused);
            }
        }
    }
    excused
}

/// The conflicts nothing excuses, and the list entries that excuse nothing.
fn audit(keymap: &Keymap) -> (Vec<Conflict>, Vec<String>) {
    let mut lists = Lists::new();
    let open = clashes(keymap)
        .into_iter()
        .filter(|conflict| !excuse(conflict, &mut lists))
        .collect();
    for bind in keymap.binds() {
        let context = base(predicate_of(bind));
        if let Some(at) = STANDALONE.iter().position(|root| *root == context) {
            lists.standalone[at] = true;
        }
    }
    (open, lists.unused())
}

fn report(open: &[Conflict]) -> Vec<String> {
    open.iter()
        .map(|c| format!("{:?} | {} | {:?} | {:?}", c.reach, c.key, c.first, c.second))
        .collect()
}

#[test]
fn guard_no_key_does_two_things_on_one_screen() {
    let (open, _) = audit(Keymap::builtin());
    assert_eq!(report(&open), Vec::<String>::new());
}

#[test]
fn guard_every_list_entry_is_still_used() {
    let (_, unused) = audit(Keymap::builtin());
    assert_eq!(unused, Vec::<String>::new());
}

fn planted(first: (&str, &str, &str), second: (&str, &str, &str)) -> Keymap {
    let one = |(command, key, context): (&str, &str, &str)| {
        format!(
            "[[bind]]\ncommand = \"{command}\"\nkeys = [\"{key}\"]\ncontext = \"{context}\"\n\n"
        )
    };
    Keymap::parse(&(one(first) + &one(second))).expect("planted keymap")
}

#[test]
fn guard_fails_for_a_planted_pair_on_one_node() {
    let keymap = planted(
        ("multi_rename.rename", "f12", "MultiRename"),
        ("dialog.next_field", "f12", "Dialog"),
    );
    let (open, _) = audit(&keymap);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].reach, Reach::Same);
}

#[test]
fn guard_fails_for_a_planted_ancestor_pair() {
    let keymap = planted(
        ("tab.next", "f12", "NavoPane"),
        ("files.open_parent", "f12", "FileList"),
    );
    let (open, _) = audit(&keymap);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].reach, Reach::Inner);
}

#[test]
fn guard_every_context_in_the_keymap_is_known() {
    let unknown: Vec<&str> = Keymap::builtin()
        .binds()
        .iter()
        .map(|bind| base(predicate_of(bind)))
        .filter(|context| *context != "!Viewer")
        .filter(|context| !knows(context) && !STANDALONE.contains(context))
        .collect();
    assert_eq!(unknown, Vec::<&str>::new());
}
