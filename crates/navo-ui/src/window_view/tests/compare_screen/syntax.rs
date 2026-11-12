//! Syntax colours in the file diff (D38, W7f): the parse runs off the UI thread and its colours
//! reach the open view only once it ends, and a reload of the same pair keeps the colours it had
//! until the new text's own parse lands.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::body::pair;
use super::{CompareKind, CompareRow, CompareSide, CompareUpdate, Rig, RowStatus, rig};
use crate::compare_diff::body::PairSyntax;
use crate::compare_diff::{DiffUpdate, PairKind, PairText};
use crate::window_view::tests::settle;

/// A line both versions of the sample Rust file share, at the same place.
const KEPT: (u32, &str) = (1, "    let s = 1;");
/// A line only the reloaded text has: past the end of the side parsed before it.
const ADDED: (u32, &str) = (3, "    let v1 = 1;");

/// Both sides of an open Rust file, with `extra` more `let` lines than the plain three.
fn rust(extra: usize) -> PairText {
    let mut lines = vec!["fn main() {".to_owned(), "    let s = 1;".to_owned()];
    for at in 0..extra {
        lines.push(format!("    let v{at} = {at};"));
    }
    lines.push("}".to_owned());
    let len = lines.len() as u32;
    let mut text = pair(len, &[1]);
    text.left.clone_from(&lines);
    text.right = lines;
    text
}

/// How many colour runs `line` gets on the left side of `pair`; 0 with no parse for that side.
fn left_runs(pair: &PairSyntax, line: (u32, &str)) -> usize {
    let colours = HighlightTheme::default_light();
    pair.left
        .as_ref()
        .map_or(0, |side| side.line_runs(line, &*colours).len())
}

/// Lists two Rust files, so a reload and another pair both have a name the parser knows.
fn files(cx: &mut TestAppContext, rig: &Rig) {
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 3,
        modified_ms: None,
    });
    let row =
        |id, name: &str| CompareRow::new(id, name.to_owned(), RowStatus::Different, (side, side));
    (rig.compare)(CompareUpdate::Rows {
        generation: 1,
        rows: vec![row(3, "main.rs"), row(4, "lib.rs")],
        progress: Vec::new(),
    });
    settle(cx, &rig.view);
}

/// Opens the selected row and returns the request id the screen asked the worker for.
fn enter(cx: &mut TestAppContext, rig: &Rig) -> u64 {
    rig.press(cx, "enter");
    rig.diff.requests().last().expect("an open pair").id
}

/// Delivers `text` as the open pair's opened update, polls it in and draws a frame, all without
/// waiting for the background parse, so the rows still show what they had before.
fn drawn(cx: &mut TestAppContext, rig: &Rig, request: u64, text: PairText) {
    (rig.diff.pair_sink())(DiffUpdate::Opened {
        request,
        kind: PairKind::Text,
        text: Some(Arc::new(text)),
    });
    rig.view.update(cx, |view, cx| view.host_poll(cx));
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("a window");
}

/// A rig with `main.rs` open and the parse of its first text landed.
fn opened(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    files(cx, &rig);
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("main.rs"));
    let request = enter(cx, &rig);
    drawn(cx, &rig, request, rust(0));
    cx.run_until_parked();
    rig
}

/// Reloads the open pair with `text`, without waiting for the parse of the new text.
fn reloaded(cx: &mut TestAppContext, rig: &Rig, text: PairText) {
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let request = *rig.diff.refreshes().last().expect("a refresh");
    drawn(cx, rig, request, text);
}

/// What the open file view's syntax slot holds: the pair `parts` hands the rows, the
/// text's own parse, and the parse kept over the last reload.
struct Slots {
    /// The parse `parts` hands the rows.
    shown: Option<Rc<PairSyntax>>,
    /// The open text's own parse, once it landed.
    ready: Option<Rc<PairSyntax>>,
    /// The parse kept from before the last reload, until the new one lands.
    provisional: Option<Rc<PairSyntax>>,
}

/// Reads the open file view's syntax slot.
fn slots(cx: &mut TestAppContext, rig: &Rig) -> Slots {
    let (shown, ready, provisional) = rig.view.read_with(cx, |view, cx| {
        (
            view.file_syntax_shown(cx),
            view.file_syntax(),
            view.file_syntax_provisional(),
        )
    });
    Slots {
        shown,
        ready,
        provisional,
    }
}

#[gpui_kit::test]
fn a_reload_keeps_the_colours_until_the_new_parse_lands(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    assert!(
        slots(cx, &rig).ready.is_some(),
        "the first text's parse landed"
    );
    reloaded(cx, &rig, rust(2));
    let Slots {
        shown,
        ready,
        provisional,
    } = slots(cx, &rig);
    assert!(
        ready.is_none(),
        "the reloaded text's own parse has not landed yet"
    );
    let shown = shown.expect("the rows still colour with the parse kept over the reload");
    let provisional = provisional.expect("and that is the provisional one");
    assert!(
        Rc::ptr_eq(&shown, &provisional),
        "the shown pair is the kept one, not a new parse"
    );
    assert!(
        left_runs(&provisional, KEPT) > 0,
        "the line both texts share keeps its syntax colours"
    );
    assert_eq!(
        left_runs(&shown, ADDED),
        0,
        "a row past the end of the provisional side draws plain"
    );
}

#[gpui_kit::test]
fn the_new_parse_takes_the_colours_over(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    reloaded(cx, &rig, rust(2));
    cx.run_until_parked();
    let Slots {
        shown,
        ready,
        provisional,
    } = slots(cx, &rig);
    assert!(
        provisional.is_none(),
        "the kept parse is dropped once the new one lands"
    );
    let ready = ready.expect("the reloaded text's own parse landed");
    let shown = shown.expect("and the rows colour with it");
    assert!(Rc::ptr_eq(&shown, &ready), "the rows use the new parse");
    assert!(
        left_runs(&ready, ADDED) > 0,
        "the line only the new text has is coloured"
    );
}

#[gpui_kit::test]
fn another_pair_opens_with_no_kept_colours(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "escape");
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("lib.rs"));
    let request = enter(cx, &rig);
    drawn(cx, &rig, request, rust(0));
    let Slots {
        shown,
        ready,
        provisional,
    } = slots(cx, &rig);
    assert!(
        shown.is_none() && provisional.is_none() && ready.is_none(),
        "a new FileScreen starts plain: nothing is kept over another pair"
    );
    cx.run_until_parked();
    assert!(
        slots(cx, &rig).shown.is_some(),
        "and its own parse is the first colours it gets"
    );
}

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
    rig.view.update(cx, |view, cx| view.host_poll(cx));
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
