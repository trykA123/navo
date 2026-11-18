//! Tests for the editor adapter core, with counting and fixed-span stub lexers.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::base::input::{HighlightStyleResolver, InputHighlighter, Rope};
use gpui_kit::{HighlightStyle, Hsla};
use navo_core::lines::{LineLexer, LineState, ROLES, Span};

use super::LineHighlighter;
use crate::languages::HIGHLIGHT_NAMES;

/// Counts calls and bytes; marks the whole chunk as `keyword`; counts lines in the state.
#[derive(Default)]
struct Counting {
    calls: AtomicUsize,
    bytes: AtomicUsize,
}

impl LineLexer for Counting {
    fn name(&self) -> &str {
        "count"
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.bytes.fetch_add(chunk.len(), Ordering::SeqCst);
        let spans = if chunk.is_empty() {
            vec![]
        } else {
            vec![(0..chunk.len(), "keyword")]
        };
        (spans, LineState(state.0 + 1))
    }
}

/// One-byte spans on every even byte.
struct Dense;

impl LineLexer for Dense {
    fn name(&self) -> &str {
        "dense"
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        (
            (0..chunk.len())
                .step_by(2)
                .map(|i| (i..i + 1, "number"))
                .collect(),
            state,
        )
    }
}

/// Counts `lex` and `lex_last` calls and bytes; claims every line starts from the default state.
#[derive(Default)]
struct Local {
    plain: AtomicUsize,
    last: AtomicUsize,
    bytes: AtomicUsize,
}

impl LineLexer for Local {
    fn name(&self) -> &str {
        "local"
    }
    fn lex(&self, _chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.plain.fetch_add(1, Ordering::SeqCst);
        (vec![], state)
    }
    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.last.fetch_add(1, Ordering::SeqCst);
        self.bytes.fetch_add(chunk.len(), Ordering::SeqCst);
        (vec![(0..chunk.len().min(1), "keyword")], state)
    }
    fn line_local(&self) -> bool {
        true
    }
}

fn setup(lexer: Arc<dyn LineLexer>, text: &str) -> LineHighlighter {
    let mut h = LineHighlighter::new(lexer);
    h.set_text(&Rope::from(text), 0);
    h
}

fn lines(n: usize) -> String {
    (0..n).map(|i| format!("l{i:05}\n")).collect()
}

#[test]
fn three_line_rope_is_covered_in_order() {
    let h = setup(Arc::new(Counting::default()), "ab\r\ncd\nef");
    let spans = h.spans(&(0..9));
    assert_eq!(
        spans,
        vec![(0..3, "keyword"), (4..6, "keyword"), (7..9, "keyword")]
    );
}

#[test]
fn range_clips_spans() {
    let h = setup(Arc::new(Counting::default()), "abcd\nefgh\n");
    assert_eq!(h.spans(&(2..7)), vec![(2..4, "keyword"), (5..7, "keyword")]);
}

#[test]
fn second_call_lexes_at_most_63_lines_before_the_range() {
    let lexer = Arc::new(Counting::default());
    let h = setup(lexer.clone(), &lines(400));
    let at = |row: usize| row * 7..row * 7 + 7;
    h.spans(&at(300));
    lexer.calls.store(0, Ordering::SeqCst);
    h.spans(&at(300));
    assert!(
        lexer.calls.load(Ordering::SeqCst) <= 64,
        "{}",
        lexer.calls.load(Ordering::SeqCst)
    );
}

#[test]
fn edit_at_row_130_keeps_marks_0_to_2() {
    let lexer = Arc::new(Counting::default());
    let mut h = setup(lexer.clone(), &lines(400));
    h.spans(&(0..400 * 7));
    assert_eq!(h.marks.borrow().len(), 7);
    h.set_text(&Rope::from(lines(400).as_str()), 130);
    assert_eq!(h.marks.borrow().len(), 3);
}

#[test]
fn deep_range_in_a_big_rope_lexes_a_bounded_stretch() {
    let lexer = Arc::new(Counting::default());
    let text = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\n"
        .repeat(3 * 1024 * 1024 / 64);
    let h = setup(lexer.clone(), &text);
    let start = 2 * 1024 * 1024;
    h.spans(&(start..start + 640));
    let bytes = lexer.bytes.load(Ordering::SeqCst);
    assert!(bytes <= 64 * 1024 + 640 + 128, "{bytes}");
    assert!(h.marks.borrow().len() <= 1, "restart stores nothing");
}

#[test]
fn single_megabyte_line_is_never_sliced() {
    let lexer = Arc::new(Counting::default());
    let h = setup(lexer.clone(), &"x".repeat(1024 * 1024));
    assert!(h.spans(&(0..1024 * 1024)).is_empty());
    assert_eq!(lexer.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn run_count_cap_applies_to_one_call() {
    let text = "a\n".repeat(70_000);
    let h = setup(Arc::new(Dense), &text);
    assert_eq!(h.spans(&(0..text.len())).len(), 65_536);
}

#[test]
fn every_role_is_a_highlight_name() {
    for role in ROLES {
        assert!(HIGHLIGHT_NAMES.contains(&role), "{role}");
    }
}

/// Marks `ERROR` as `title` and `zzz` as an unknown role.
struct ErrorLexer;

impl LineLexer for ErrorLexer {
    fn name(&self) -> &str {
        "errs"
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        let mut spans = Vec::new();
        if chunk.starts_with("ERROR") {
            spans.push((0..5, "title"));
        }
        if let Some(at) = chunk.find("zzz") {
            spans.push((at..at + 3, "keyword"));
        }
        (spans, state)
    }
}

/// Gives `title` a red colour and knows nothing else.
struct TitleOnly;

impl HighlightStyleResolver for TitleOnly {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        (name == "title").then(|| HighlightStyle {
            color: Some(Hsla {
                h: 0.,
                s: 1.,
                l: 0.5,
                a: 1.,
            }),
            ..HighlightStyle::default()
        })
    }
}

/// Gives every role the same red colour.
struct AnyRole;

impl HighlightStyleResolver for AnyRole {
    fn style(&self, _name: &str) -> Option<HighlightStyle> {
        TitleOnly.style("title")
    }
}

fn title() -> HighlightStyle {
    TitleOnly.style("title").expect("title style")
}

#[test]
fn error_gets_the_title_style_and_the_rest_is_default() {
    let h = setup(Arc::new(ErrorLexer), "ERROR x");
    let runs = h.styles(&(0..7), &TitleOnly);
    assert_eq!(
        runs,
        vec![(0..5, title()), (5..7, HighlightStyle::default())]
    );
}

#[test]
fn runs_cover_a_partial_range_exactly_and_unknown_roles_are_default() {
    let h = setup(Arc::new(ErrorLexer), "ERROR zzz\nplain");
    let runs = h.styles(&(3..14), &TitleOnly);
    let expected = vec![(3..5, title()), (5..14, HighlightStyle::default())];
    assert_eq!(runs, expected);
}

#[test]
fn language_is_the_lexer_name_and_there_are_no_folds() {
    let h = setup(Arc::new(ErrorLexer), "ERROR");
    assert_eq!(h.language().as_ref(), "errs");
    assert!(h.fold_ranges(&Rope::from("ERROR")).is_empty());
}

#[test]
fn factory_serves_any_language_name() {
    let factory = LineHighlighter::factory(Arc::new(ErrorLexer));
    assert_eq!(
        factory("anything")
            .expect("highlighter")
            .language()
            .as_ref(),
        "errs"
    );
}

#[test]
fn every_editor_line_is_lexed_as_a_chunk_that_ends_its_line() {
    let lexer = Arc::new(Local::default());
    let h = setup(lexer.clone(), "ab\ncd\nef");
    h.spans(&(0..8));
    assert_eq!(lexer.last.load(Ordering::SeqCst), 3);
    assert_eq!(lexer.plain.load(Ordering::SeqCst), 0);
}

#[test]
fn a_line_local_lexer_lexes_only_the_rows_in_range_and_stores_no_marks() {
    let lexer = Arc::new(Local::default());
    let text = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\n"
        .repeat(3 * 1024 * 1024 / 64);
    let h = setup(lexer.clone(), &text);
    let start = 2 * 1024 * 1024;
    let spans = h.spans(&(start..start + 640));
    assert_eq!(spans.len(), 10);
    assert_eq!(lexer.last.load(Ordering::SeqCst), 10);
    assert_eq!(lexer.bytes.load(Ordering::SeqCst), 10 * 63);
    assert!(h.marks.borrow().is_empty(), "no checkpoints are needed");
}

#[test]
fn styles_never_returns_more_than_the_run_cap_and_covers_the_range_exactly() {
    let text = "a\n".repeat(70_000);
    let h = setup(Arc::new(Dense), &text);
    let runs = h.styles(&(0..text.len()), &AnyRole);
    assert!(runs.len() <= 65_536, "{}", runs.len());
    let mut at = 0;
    for (run, _) in &runs {
        assert_eq!(run.start, at);
        assert!(run.end > run.start);
        at = run.end;
    }
    assert_eq!(at, text.len());
    let (tail, style) = runs.last().expect("runs");
    assert_eq!(*style, HighlightStyle::default());
    assert!(tail.len() > 1, "the last run covers the rest");
}

#[test]
fn the_last_styled_span_that_fits_the_run_cap_exactly_is_kept() {
    // A leading gap, then 32_768 lines of "a\n": gap and span alternate, 65_536 runs, no tail.
    let text = format!("\n{}", "a\n".repeat(32_768));
    let h = setup(Arc::new(Dense), &text);
    let end = text.len() - 1;
    let runs = h.styles(&(0..end), &AnyRole);
    assert_eq!(runs.len(), 65_536);
    assert_eq!(runs[runs.len() - 1], (end - 1..end, title()));
    assert_eq!(
        runs[runs.len() - 2],
        (end - 2..end - 1, HighlightStyle::default())
    );
}
