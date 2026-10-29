use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::{TreeBody, TreeState, TreeView};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in, viewer_open};
use crate::window_view::viewer::tests::structured::{big_xml, open_named};
use crate::window_view::viewer::tests::{Setup, has_editor, label, redraw, text_of};

const NESTED: &[u8] =
    br#"{"a": {"b": {"c": {"d": {"e": {"f": {"g": {"h": {"i": {"j": {"k": 1}}}}}}}}}}, "z": [1, 2]}"#;

fn tree_view(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Entity<TreeView> {
    view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        Some(body.as_any().downcast_ref::<TreeBody>()?.0.clone())
    })
    .expect("a tree view is open")
}

fn shared_tree(cx: &mut TestAppContext, tree: &Entity<TreeView>) -> Arc<rusty_structured::Tree> {
    tree.read_with(cx, |tree, _| match &tree.state {
        TreeState::Ready(ready) => ready.tree.clone(),
        _ => panic!("the index is ready"),
    })
}

#[gpui_kit::test]
fn children_wait_for_a_running_filter_instead_of_copying_the_tree(cx: &mut TestAppContext) {
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(big_xml())
    };
    let (window, view, _) = open_named(cx, "big.arxml", setup);
    let tree = tree_view(cx, &view);
    let held = shared_tree(cx, &tree);
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "right");
    assert!(!label(cx, window, ("tree-row", 3)).starts_with("ELEMENT"));
    let original = Arc::as_ptr(&held);
    drop(held);
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    redraw(cx, window);
    assert!(label(cx, window, ("tree-row", 3)).starts_with("ELEMENT"));
    assert!(std::ptr::eq(Arc::as_ptr(&shared_tree(cx, &tree)), original));
}

struct Opened {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    tree: Entity<TreeView>,
}

impl Opened {
    fn json(cx: &mut TestAppContext, bytes: &[u8]) -> Self {
        let (window, view, _) = open_named(cx, "nested.json", Setup::new(bytes.to_vec()));
        let tree = tree_view(cx, &view);
        Self { window, view, tree }
    }

    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn rows(&self, cx: &mut TestAppContext) -> usize {
        self.tree.read_with(cx, |tree, _| {
            tree.ready().map_or(0, |ready| ready.outline.rows().len())
        })
    }

    fn path(&self, cx: &mut TestAppContext) -> String {
        text_of(cx, self.window, "tree-path")
    }
}

#[gpui_kit::test]
fn alt_right_opens_the_node_with_everything_under_it(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["down", "alt-right"]);
    assert_eq!(tree.rows(cx), 13);
    assert_eq!(tree.path(cx), "$.a");
}

#[gpui_kit::test]
fn alt_left_closes_the_node_with_everything_under_it_and_stays_in_the_tree(
    cx: &mut TestAppContext,
) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["down", "right", "right", "right", "up"]);
    assert_eq!(tree.rows(cx), 5);
    tree.press(cx, &["alt-left"]);
    assert_eq!(tree.rows(cx), 3);
    assert_eq!(tree.path(cx), "$.a");
    assert!(viewer_open(cx, &tree.view));
    tree.press(cx, &["right"]);
    assert_eq!(tree.rows(cx), 4);
}

#[gpui_kit::test]
fn ctrl_shift_right_opens_the_whole_tree(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["ctrl-shift-right"]);
    assert_eq!(tree.rows(cx), 15);
    assert_eq!(tree.path(cx), "$");
}

#[gpui_kit::test]
fn ctrl_shift_left_closes_the_whole_tree(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["down", "right", "end", "right"]);
    assert_eq!(tree.rows(cx), 6);
    tree.press(cx, &["ctrl-shift-left"]);
    assert_eq!(tree.rows(cx), 3);
    assert_eq!(tree.path(cx), "$.z");
}

#[gpui_kit::test]
fn ctrl_1_shows_only_the_children_of_the_root(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["down", "right"]);
    assert_eq!(tree.rows(cx), 4);
    tree.press(cx, &["ctrl-1"]);
    assert_eq!(tree.rows(cx), 3);
    assert!(label(cx, tree.window, ("tree-row", 2)).starts_with('z'));
}

#[gpui_kit::test]
fn ctrl_3_shows_the_tree_three_levels_deep(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["ctrl-shift-right", "ctrl-3"]);
    assert_eq!(tree.rows(cx), 7);
    assert!(label(cx, tree.window, ("tree-row", 3)).starts_with('c'));
}

#[gpui_kit::test]
fn ctrl_9_shows_the_tree_nine_levels_deep(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["ctrl-9"]);
    assert_eq!(tree.rows(cx), 13);
    assert!(label(cx, tree.window, ("tree-row", 9)).starts_with('i'));
}

#[gpui_kit::test]
fn a_depth_fold_keeps_the_cursor_or_moves_it_to_the_nearest_shown_ancestor(
    cx: &mut TestAppContext,
) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["ctrl-shift-right", "end", "ctrl-3"]);
    assert_eq!(tree.path(cx), "$.z[1]");
    tree.press(cx, &["ctrl-shift-right"]);
    tree.press(cx, &["home"]);
    tree.press(cx, &["down"; 11]);
    assert_eq!(tree.path(cx), "$.a.b.c.d.e.f.g.h.i.j.k");
    tree.press(cx, &["ctrl-3"]);
    assert_eq!(tree.path(cx), "$.a.b.c");
}

#[gpui_kit::test]
fn opening_and_folding_a_200_000_node_tree_stays_responsive(cx: &mut TestAppContext) {
    let items = vec![r#"{"a": {"b": [1, 2]}}"#; 40_000].join(",");
    let tree = Opened::json(cx, format!("[{items}]").as_bytes());
    let started = Instant::now();
    tree.press(cx, &["ctrl-shift-right"]);
    assert_eq!(tree.rows(cx), 200_001);
    tree.press(cx, &["ctrl-1"]);
    assert_eq!(tree.rows(cx), 40_001);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[gpui_kit::test]
fn opening_an_indexed_xml_tree_reads_nothing_more(cx: &mut TestAppContext) {
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(big_xml())
    };
    let (window, view, log) = open_named(cx, "big.arxml", setup);
    log.lock().expect("reads").clear();
    press(cx, window, &view, "ctrl-shift-right");
    press(cx, window, &view, "ctrl-9");
    assert!(log.lock().expect("reads").is_empty());
    assert_ne!(text_of(cx, window, "tree-status"), "Loading…");
}

fn find_query(cx: &mut TestAppContext, tree: &Opened) -> String {
    tree.tree.read_with(cx, |tree, cx| {
        tree.find
            .as_ref()
            .map(|find| find.input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_find_match_inside_a_folded_node_opens_its_ancestors_and_selects_it(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    assert_eq!(tree.rows(cx), 3);
    tree.press(cx, &["ctrl-f"]);
    type_in(cx, tree.window, &tree.view, "k");
    assert_eq!(tree.path(cx), "$.a.b.c.d.e.f.g.h.i.j.k");
    assert_eq!(text_of(cx, tree.window, "find-count"), "1 of 1");
    assert_eq!(tree.rows(cx), 13);
}

#[gpui_kit::test]
fn digits_typed_into_the_find_bar_stay_in_the_field_and_keep_the_tree(cx: &mut TestAppContext) {
    let tree = Opened::json(cx, NESTED);
    tree.press(cx, &["ctrl-f", "1", "3"]);
    assert_eq!(find_query(cx, &tree), "13");
    assert!(!has_editor(cx, &tree.view));
    assert_eq!(text_of(cx, tree.window, "find-count"), "No matches");
    assert!(viewer_open(cx, &tree.view));
}
