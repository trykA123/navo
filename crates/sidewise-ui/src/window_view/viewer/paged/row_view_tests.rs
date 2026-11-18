//! Tests for the paged row highlight runs (syntax colours versus hit and contrast rows).

use gpui_kit::component::highlighter::HighlightTheme;
use navo_core::paging::TextLayout;

use super::{RowLook, TextLook, row_highlights};
use crate::tokens::{Mode, Theme};
use crate::window_view::viewer::paged::lines::TextLine;
use crate::window_view::viewer::paged::spans::Painted;

fn runs(is_hit: bool, is_contrast: bool, syntax: bool) -> usize {
    let mut t = Theme::new(Mode::Dark);
    t.is_contrast = is_contrast;
    let look = RowLook {
        t: &t,
        height: crate::prelude::px(20.),
        painted: Painted {
            hit: None,
            marks: &[],
            matches: &[],
        },
        bookmarks: &[],
        line_ends: false,
    };
    let text = TextLook {
        digits: 1,
        shift: 0,
        numbers: false,
        layout: TextLayout::default(),
        syntax: syntax.then(HighlightTheme::default_dark),
    };
    let line = TextLine {
        start: 0,
        text_from: 0,
        end: 3,
        next: 3,
        text: "abc".into(),
        spans: vec![(0..3, "keyword")],
        line: Some(1),
    };
    row_highlights(&look, &text, &line, is_hit).1.len()
}

#[test]
fn syntax_runs_skip_the_hit_row_and_contrast_themes() {
    assert_eq!(runs(false, false, true), 1);
    assert_eq!(runs(true, false, true), 0);
    assert_eq!(runs(false, true, true), 0);
    assert_eq!(runs(false, false, false), 0);
}
