use super::contexts::{GLOBAL, parent};

/// How two key contexts relate on the real screen, seen from the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    Same,
    Inner,
    Outer,
}

/// Parents that exist on screen but not in `PARENTS`, which only decides where a user may bind.
/// A child may list several parents: the find bar and Find All render inside the text and hex views.
const SCREEN_PARENTS: [(&str, &str); 8] = [
    ("FileList", "NavoPane"),
    ("Compare", "FileList"),
    ("PaneWaiting", "NavoPane"),
    ("Viewer", "PreviewBrowse"),
    ("ViewerFind", "Paged"),
    ("ViewerFind", "Editor"),
    ("FindAll", "Paged"),
    ("FindAll", "Editor"),
];

/// Contexts that one element carries together.
const SHARED: [(&str, &str); 3] = [
    ("Elf", "Space"),
    ("Map", "Space"),
    ("MultiRename", "Dialog"),
];

/// Contexts that are a root on screen although `PARENTS` nests them.
const ROOTS: [&str; 2] = ["CompareView", "CompareStream"];

/// True when the model has a fact about the context (a slot counts by its outermost part).
#[cfg(test)]
pub(crate) fn knows(context: &str) -> bool {
    let base = context.split(" > ").next().unwrap_or(context);
    let named = |pair: &[(&str, &str)]| pair.iter().any(|&(x, y)| x == base || y == base);
    base == GLOBAL
        || ROOTS.contains(&base)
        || named(&SCREEN_PARENTS)
        || named(&SHARED)
        || named(&super::contexts::PARENTS)
}

pub(crate) fn same_node(a: &str, b: &str) -> bool {
    a == b
        || SHARED
            .iter()
            .any(|&(x, y)| (x, y) == (a, b) || (y, x) == (a, b))
}

fn mates(context: &str) -> Vec<&'static str> {
    SHARED
        .iter()
        .filter_map(|&(x, y)| {
            if x == context {
                Some(y)
            } else {
                (y == context).then_some(x)
            }
        })
        .collect()
}

fn screen_parents(context: &str) -> Vec<String> {
    let mut found = Vec::new();
    for at in std::iter::once(context).chain(mates(context)) {
        for (_, outer) in SCREEN_PARENTS.iter().filter(|(child, _)| *child == at) {
            found.push((*outer).to_owned());
        }
        if !ROOTS.contains(&at)
            && let Some(outer) = parent(at)
        {
            found.push(outer.to_owned());
        }
    }
    found
}

fn is_inside(context: &str, home: &str) -> bool {
    let mut seen: Vec<String> = Vec::new();
    let mut todo = screen_parents(context);
    while let Some(at) = todo.pop() {
        if at == home {
            return true;
        }
        if !seen.contains(&at) {
            todo.extend(screen_parents(&at));
            seen.push(at);
        }
    }
    false
}

/// `Outer` when `mine` sits inside `theirs`, `Inner` when `theirs` sits inside `mine`.
/// A context-free key is live on every node, so it is `Same` with anything.
pub(crate) fn screen_reach(mine: &str, theirs: &str) -> Option<Reach> {
    if mine == GLOBAL || theirs == GLOBAL || same_node(mine, theirs) {
        Some(Reach::Same)
    } else if is_inside(mine, theirs) {
        Some(Reach::Outer)
    } else if is_inside(theirs, mine) {
        Some(Reach::Inner)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_paths_file_list_sits_inside_the_pane() {
        assert_eq!(screen_reach("FileList", "NavoPane"), Some(Reach::Outer));
        assert_eq!(screen_reach("NavoPane", "FileList"), Some(Reach::Inner));
        assert_eq!(screen_reach("PaneWaiting", "NavoPane"), Some(Reach::Outer));
        assert_eq!(screen_reach("Compare", "NavoPane"), Some(Reach::Outer));
    }

    #[test]
    fn focus_paths_the_preview_body_is_a_viewer_inside_the_browse_node() {
        assert_eq!(screen_reach("Viewer", "PreviewBrowse"), Some(Reach::Outer));
        assert_eq!(screen_reach("PreviewBrowse", "Viewer"), Some(Reach::Inner));
        assert_eq!(screen_reach("Image", "PreviewBrowse"), Some(Reach::Outer));
        assert_eq!(screen_reach("PreviewBrowse", "Image"), Some(Reach::Inner));
        assert!(!is_inside("PreviewBrowse", "Viewer"));
        assert!(!is_inside("PreviewBrowse", "Image"));
    }

    #[test]
    fn focus_paths_contexts_of_one_element_are_the_same_node() {
        assert_eq!(screen_reach("Elf", "Space"), Some(Reach::Same));
        assert_eq!(screen_reach("Map", "Space"), Some(Reach::Same));
        assert_eq!(screen_reach("MultiRename", "Dialog"), Some(Reach::Same));
        assert_eq!(screen_reach("Elf", "Map"), None);
    }

    #[test]
    fn focus_paths_the_compare_node_sits_inside_the_list() {
        assert_eq!(screen_reach("Compare", "FileList"), Some(Reach::Outer));
        assert_eq!(screen_reach("FileList", "Compare"), Some(Reach::Inner));
    }

    #[test]
    fn focus_paths_a_shared_node_keeps_its_own_ancestors() {
        assert_eq!(screen_reach("ElfTable", "Space"), Some(Reach::Outer));
        assert_eq!(screen_reach("ElfTable", "Map"), None);
        assert_eq!(screen_reach("MapTable", "Elf"), None);
        assert_eq!(
            screen_reach("MultiRenamePreview", "Dialog"),
            Some(Reach::Outer)
        );
        assert_eq!(screen_reach("Compare", "Viewer"), None);
    }

    #[test]
    fn focus_paths_the_file_diff_is_a_root() {
        assert_eq!(screen_reach("CompareView", "Compare"), None);
        assert_eq!(screen_reach("Compare", "CompareView"), None);
        assert_eq!(screen_reach("CompareView", "FileList"), None);
    }

    #[test]
    fn focus_paths_a_context_free_key_is_live_everywhere() {
        for context in [
            "FileList",
            "Image",
            "CompareView",
            "ClipPalette > Input",
            GLOBAL,
        ] {
            assert_eq!(screen_reach(GLOBAL, context), Some(Reach::Same));
            assert_eq!(screen_reach(context, GLOBAL), Some(Reach::Same));
        }
    }

    #[test]
    fn focus_paths_a_slot_is_an_input_under_its_outer_context() {
        assert_eq!(
            screen_reach("ClipPalette > Input", "ClipPalette"),
            Some(Reach::Outer)
        );
        assert_eq!(
            screen_reach("ClipPalette", "ClipPalette > Input"),
            Some(Reach::Inner)
        );
        assert_eq!(screen_reach("ClipPalette > Input", "CommandCenter"), None);
    }

    #[test]
    fn focus_paths_the_find_bar_and_find_all_sit_inside_paged_and_editor() {
        for find in ["ViewerFind", "FindAll"] {
            for screen in ["Paged", "Editor", "Viewer"] {
                assert_eq!(
                    screen_reach(find, screen),
                    Some(Reach::Outer),
                    "{find} {screen}"
                );
                assert_eq!(
                    screen_reach(screen, find),
                    Some(Reach::Inner),
                    "{screen} {find}"
                );
            }
        }
        assert_eq!(screen_reach("Paged", "Editor"), None);
        assert_eq!(screen_reach("Image", "ViewerFind"), None);
    }

    #[test]
    fn focus_paths_the_logical_nesting_still_holds() {
        assert_eq!(screen_reach("Image", "Viewer"), Some(Reach::Outer));
        assert_eq!(screen_reach("Viewer", "Pdf"), Some(Reach::Inner));
        assert_eq!(screen_reach("Pdf", "Image"), None);
    }
}
