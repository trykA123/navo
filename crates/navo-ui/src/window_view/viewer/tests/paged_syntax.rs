//! Paged text colour tests: load, rendered runs, Hex and Follow reuse, page cuts, binary files.
//! They drive the viewer by keys and read `row_spans` and the `text_row` run probe.

use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use sidewise_ui::window_view::viewer::paged::run_probe;

use super::structured::fake_with;
use super::{Log, Setup, hooks, kind, paged, press_only, redraw, window_with};
use crate::window_view::NavoWindow;
use crate::window_view::tests::press;
use crate::window_view::viewer::{EDIT_LIMIT, Kinds, PagedKind};

const ONE_LINE: &str = "<A b=\"1\"><!-- c --></A>\n";

/// Opens `name` with `bytes` as a file past the edit limit; F3 shows the Tree first (D18).
fn open_big(
    cx: &mut TestAppContext,
    name: &str,
    bytes: Vec<u8>,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let files = hooks(Setup::huge(bytes), &Log::default());
    let fs = fake_with(name, EDIT_LIMIT + 1);
    let (window, view) = window_with(cx, fs, files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    redraw(cx, window);
    (window, view)
}

/// Presses a key and draws the frame it caused.
fn key(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    press_only(cx, window, name);
    redraw(cx, window);
}

/// Roles of the filled row `index`, in order.
fn roles(cx: &mut TestAppContext, view: &Entity<NavoWindow>, index: usize) -> Vec<&'static str> {
    let view = paged(cx, view);
    let spans = view.read_with(cx, |view, _| view.row_spans(index));
    spans.into_iter().map(|(_, role)| role).collect()
}

/// The role lists of the first `rows` filled rows.
fn screen_roles(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<Vec<&'static str>> {
    let rows = paged(cx, view).read_with(cx, |paged, _| paged.rows);
    (0..rows as usize).map(|row| roles(cx, view, row)).collect()
}

/// Whether the drawn row that starts with `<A b="1">` has a run in the theme's `tag` style.
fn has_tag_run(cx: &mut TestAppContext) -> bool {
    let tag = cx.read(|cx| KitTheme::global(cx).highlight_theme.style("tag"));
    let rows = run_probe::take();
    let row = rows
        .iter()
        .rev()
        .find(|row| row.shown.starts_with("<A b=\"1\">"))
        .expect("the probe saw the row");
    let tag = tag.expect("the theme styles tag");
    row.runs.iter().any(|(_, style)| *style == tag)
}

#[gpui_kit::test]
fn a_big_arxml_opens_as_tree_and_alt_1_shows_coloured_text(cx: &mut TestAppContext) {
    let (window, view) = open_big(cx, "big.arxml", ONE_LINE.as_bytes().to_vec());
    assert_eq!(kind(cx, &view), None, "F3 opens the Tree, not paged text");
    key(cx, window, "alt-1");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    let found = roles(cx, &view, 0);
    for role in ["tag", "attribute", "string", "comment"] {
        assert!(found.contains(&role), "{role}: {found:?}");
    }
}

#[gpui_kit::test]
fn a_redraw_hands_the_row_a_tag_run_until_the_lexer_is_removed(cx: &mut TestAppContext) {
    let (window, view) = open_big(cx, "big.arxml", ONE_LINE.as_bytes().to_vec());
    key(cx, window, "alt-1");
    run_probe::take();
    redraw(cx, window);
    assert!(has_tag_run(cx), "coloured row");
    let paged = paged(cx, &view);
    paged.update(cx, |view, cx| view.set_lexer(None, cx));
    redraw(cx, window);
    assert!(!has_tag_run(cx), "plain row");
}

#[gpui_kit::test]
fn hex_then_text_keeps_the_colours(cx: &mut TestAppContext) {
    let (window, view) = open_big(cx, "big.arxml", ONE_LINE.as_bytes().to_vec());
    key(cx, window, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    key(cx, window, "alt-1");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert!(roles(cx, &view, 0).contains(&"tag"));
}

#[gpui_kit::test]
fn the_editor_followed_as_paged_text_keeps_the_colours(cx: &mut TestAppContext) {
    let bytes = ONE_LINE.as_bytes().to_vec();
    let len = bytes.len() as u64;
    let files = hooks(Setup::new(bytes), &Log::default());
    let (window, view) = window_with(cx, fake_with("app.xml", len), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    redraw(cx, window);
    key(cx, window, "alt-1");
    key(cx, window, "alt-f");
    assert!(roles(cx, &view, 0).contains(&"tag"));
}

#[gpui_kit::test]
fn a_comment_cut_by_a_4_kib_row_stays_a_comment(cx: &mut TestAppContext) {
    let text = format!("<!--{}-->\n<A/>\n", "x".repeat(5000));
    let (window, view) = open_big(cx, "big.arxml", text.into_bytes());
    key(cx, window, "alt-1");
    assert_eq!(roles(cx, &view, 0), ["comment"]);
    assert_eq!(roles(cx, &view, 1), ["comment"]);
    assert!(roles(cx, &view, 2).contains(&"tag"));
}

/// Lines that open and close comments, so a page's colours depend on the state before it.
fn comment_lines(count: usize) -> Vec<u8> {
    (0..count)
        .map(|at| match at % 3 {
            0 => "<A b=\"1\">\n",
            1 => "<!-- opens\n",
            _ => "closes --></A>\n",
        })
        .collect::<String>()
        .into_bytes()
}

#[gpui_kit::test]
fn page_down_up_down_gives_the_same_colours_on_the_same_rows(cx: &mut TestAppContext) {
    let (window, view) = open_big(cx, "big.arxml", comment_lines(600));
    key(cx, window, "alt-1");
    key(cx, window, "pagedown");
    let (top, first) = (super::top(cx, &view), screen_roles(cx, &view));
    assert!(top > 0, "the page moved");
    assert!(first.iter().any(|row| row.contains(&"comment")));
    key(cx, window, "pageup");
    key(cx, window, "pagedown");
    assert_eq!(super::top(cx, &view), top);
    assert_eq!(screen_roles(cx, &view), first);
}

#[gpui_kit::test]
fn ctrl_end_inside_a_comment_colours_the_last_screen_as_a_comment(cx: &mut TestAppContext) {
    let mut text = "<A/>\n".repeat(100) + "<!-- long comment\n";
    text += &"still inside the comment\n".repeat(300);
    text += "done -->\n<A/>\n";
    let (window, view) = open_big(cx, "big.arxml", text.into_bytes());
    key(cx, window, "alt-1");
    key(cx, window, "ctrl-end");
    let last = screen_roles(cx, &view);
    assert!(last.len() > 10);
    // The page starts inside the comment, so its head resyncs on the closing `-->`.
    assert!(last[..10].iter().all(|row| row == &["comment"]), "{last:?}");
    assert!(last.iter().any(|row| row.contains(&"tag")), "{last:?}");
}

#[gpui_kit::test]
fn a_binary_file_shown_as_text_has_no_colours(cx: &mut TestAppContext) {
    let (window, view) = open_big(cx, "big.bin", super::mixed(4096));
    key(cx, window, "alt-1");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert!(screen_roles(cx, &view).iter().all(Vec::is_empty));
}
