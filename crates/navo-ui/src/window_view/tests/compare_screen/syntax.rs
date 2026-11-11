//! Syntax colours in the file diff (D38): the parse runs off the UI thread and its colours
//! reach the open view only once it ends.

use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::component::highlighter::HighlightTheme;

use super::body::pair;
use super::{CompareKind, CompareRow, CompareSide, CompareUpdate, RowStatus, rig};
use crate::compare_diff::{DiffUpdate, PairKind};
use crate::window_view::tests::settle;

#[gpui_kit::test]
fn compare_screen_syntax_arrives_after_the_background_parse(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 3,
        modified_ms: None,
    });
    let row = CompareRow::new(3, "main.rs".to_owned(), RowStatus::Different, (side, side));
    (rig.compare)(CompareUpdate::Rows {
        generation: 1,
        rows: vec![row],
        progress: Vec::new(),
    });
    settle(cx, &rig.view);
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("main.rs"));
    rig.press(cx, "enter");
    let mut text = pair(3, &[1]);
    text.left = vec!["fn main() {".into(), "    let s = 1;".into(), "}".into()];
    text.right.clone_from(&text.left);
    (rig.diff.pair_sink())(DiffUpdate::Opened {
        request: rig.diff.requests()[0].id,
        kind: PairKind::Text,
        text: Some(Arc::new(text)),
    });
    rig.view.update(cx, |view, cx| view.poll_compare_screen(cx));
    let parsed = rig.view.read_with(cx, |view, _| view.file_syntax());
    assert!(parsed.is_none(), "the poll returns before the parse ends");
    cx.run_until_parked();
    let parsed = rig.view.read_with(cx, |view, _| view.file_syntax());
    let left = parsed.as_ref().and_then(|pair| pair.left.as_ref());
    let colours = HighlightTheme::default_light();
    let runs = left.map(|side| side.line_runs((1, "    let s = 1;"), &*colours));
    assert!(
        runs.is_some_and(|runs| !runs.is_empty()),
        "the Rust side has colour runs once parsed"
    );
}
