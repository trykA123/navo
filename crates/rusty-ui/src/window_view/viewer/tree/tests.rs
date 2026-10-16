use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};

use super::{TreeBody, TreeState, TreeView};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::tests::structured::{big_xml, open_named};
use crate::window_view::viewer::tests::{Setup, label, redraw};

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
