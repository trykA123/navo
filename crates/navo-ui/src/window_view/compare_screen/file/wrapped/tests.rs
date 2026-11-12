//! Tests for the wrapped cut: every visual line carries the widths its checkpoint comes from.

use std::sync::Arc;

use super::WrapDraw;
use crate::compare_diff::hooks::{DiffRow, HunkView, PairFacts, ViewedKey};
use crate::compare_diff::{LineCols, PairText, WrapIndex};

/// The measured widths of a pair whose only left line is `line`.
fn widths(line: &str) -> Arc<LineCols> {
    let text = PairText {
        left: vec![line.to_owned()],
        right: Vec::new(),
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        rows: Vec::<DiffRow>::new(),
        hunks: Vec::<HunkView>::new(),
        facts: PairFacts {
            left_encoding: "",
            right_encoding: "",
            left_endings: "",
            right_endings: "",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    };
    Arc::new(LineCols::measure(&text))
}

#[test]
fn a_wrapped_cut_carries_the_widths_of_its_text() {
    let widths = widths(&"x".repeat(6000));
    let draw = WrapDraw {
        index: WrapIndex::flat(1),
        cols: 100,
        widths: Arc::clone(&widths),
    };
    let cut = draw.cut(50);
    assert_eq!(cut.cols, Some(5000..5100));
    assert!(cut.continued, "the 51st visual line continues its row");
    let carried = cut.widths.expect("the cut carries the widths");
    assert!(Arc::ptr_eq(&carried, &widths), "the widths of this text");
    assert!(
        carried.left_start(0, 5000).is_some(),
        "a checkpoint below column 5000"
    );
}
