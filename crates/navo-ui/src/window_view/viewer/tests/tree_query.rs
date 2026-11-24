//! Tests for the tree view's query bar (Ctrl+Shift+F) and its breadcrumb.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_structured::json::parse_json;
use navo_structured::tree::Query;

use super::structured::open_named;
use super::{Setup, label, redraw, text_of, visible};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, settle, type_in, viewer_open};

const STORE: &[u8] = br#"{"store": {"book": [{"title": "A", "author": "X"}, {"title": "B", "author": "Y"}], "bike": {"color": "red"}}}"#;

struct Opened {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
}

impl Opened {
    fn store(cx: &mut TestAppContext) -> Self {
        let (window, view, _) = open_named(cx, "store.json", Setup::new(STORE.to_vec()));
        Self { window, view }
    }

    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    /// Types `text` into the focused field and waits past the query debounce.
    fn query(&self, cx: &mut TestAppContext, text: &str) {
        type_in(cx, self.window, &self.view, text);
        cx.executor().advance_clock(Duration::from_millis(500));
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    fn path(&self, cx: &mut TestAppContext) -> String {
        text_of(cx, self.window, "tree-path")
    }

    fn click_crumb(&self, cx: &mut TestAppContext, index: usize) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(("tree-crumb", index), cx);
        })
        .expect("window");
        settle(cx, &self.view);
    }
}

/// Returns `Tree::path()` of the single node `query` selects in `STORE`.
fn path_of(query: &str) -> String {
    let tree = parse_json(STORE).expect("valid JSON");
    let query = Query::parse(query).expect("valid query");
    let hits = tree.select(&query);
    assert_eq!(hits.len(), 1, "{query:?} selects one node");
    tree.path(hits[0])
}

#[gpui_kit::test]
fn a_query_selects_and_reveals_the_second_match_on_enter(cx: &mut TestAppContext) {
    let tree = Opened::store(cx);
    tree.press(cx, &["ctrl-shift-f"]);
    tree.query(cx, "$..title");
    assert_eq!(tree.path(cx), "$.store.book[0].title");
    tree.press(cx, &["enter"]);
    assert_eq!(tree.path(cx), "$.store.book[1].title");
    assert_eq!(text_of(cx, tree.window, "tree-query-count"), "2 of 2");
    assert!(label(cx, tree.window, ("tree-row", 7)).starts_with("title"));
}

#[gpui_kit::test]
fn a_bad_query_shows_the_error_column(cx: &mut TestAppContext) {
    let tree = Opened::store(cx);
    tree.press(cx, &["ctrl-shift-f"]);
    tree.query(cx, "$.store[x");
    let error = text_of(cx, tree.window, "tree-query-error");
    assert!(error.contains("column 8"), "{error:?}");
    assert_eq!(tree.path(cx), "$");
}

#[gpui_kit::test]
fn a_breadcrumb_click_selects_the_ancestor(cx: &mut TestAppContext) {
    let tree = Opened::store(cx);
    tree.press(cx, &["down", "right", "down", "right", "down"]);
    assert_eq!(tree.path(cx), "$.store.book[0]");
    tree.click_crumb(cx, 1);
    assert_eq!(tree.path(cx), "$.store");
}

#[gpui_kit::test]
fn copy_path_after_a_breadcrumb_click_copies_the_tree_path(cx: &mut TestAppContext) {
    let tree = Opened::store(cx);
    tree.press(cx, &["down", "right", "down", "right", "down"]);
    tree.click_crumb(cx, 2);
    tree.press(cx, &["ctrl-shift-c"]);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(path_of("$.store.book")));
}

#[gpui_kit::test]
fn shift_enter_steps_back_and_escape_closes_the_query_bar(cx: &mut TestAppContext) {
    let tree = Opened::store(cx);
    tree.press(cx, &["ctrl-shift-f"]);
    tree.query(cx, "$..author");
    tree.press(cx, &["shift-enter"]);
    assert_eq!(tree.path(cx), "$.store.book[1].author");
    tree.press(cx, &["escape"]);
    assert!(!visible(cx, tree.window, "tree-query"));
    assert_eq!(tree.path(cx), "$.store.book[1].author");
    assert!(viewer_open(cx, &tree.view));
}
