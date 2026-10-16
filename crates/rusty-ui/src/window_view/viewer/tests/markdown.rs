use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, ScrollDelta, TestAppContext, point, px};

use super::structured::open_named;
use super::{Setup, editor_text, has_editor, redraw, text_of};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::press;
use crate::window_view::viewer::markdown::MarkdownBody;

fn with_body<T>(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    read: impl FnOnce(&MarkdownBody, &gpui_kit::App) -> T,
) -> T {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let markdown = body.as_any().downcast_ref::<MarkdownBody>()?;
        Some(read(markdown, cx))
    })
    .expect("a markdown view is open")
}

fn markdown_text(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    with_body(cx, view, MarkdownBody::rendered)
}

fn open(
    cx: &mut TestAppContext,
    name: &str,
    source: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(source.as_bytes().to_vec()));
    (window, view)
}

#[gpui_kit::test]
fn f3_on_markdown_renders_the_heading_and_1_shows_the_source(cx: &mut TestAppContext) {
    let source = "# Release notes\n\nSome *quiet* text and a [link](https://example.com).\n";
    let (window, view) = open(cx, "notes.md", source);
    let rendered = markdown_text(cx, &view);
    assert!(rendered.starts_with("Release notes"), "{rendered}");
    assert!(
        !rendered.contains('#') && !rendered.contains('*'),
        "{rendered}"
    );
    assert_eq!(text_of(cx, window, "viewer-title"), "notes.md");
    press(cx, window, &view, "1");
    assert!(has_editor(cx, &view));
    assert!(editor_text(cx, &view).starts_with("# Release notes"));
}

#[gpui_kit::test]
fn a_normal_readme_renders_tables_lists_links_code_and_images(cx: &mut TestAppContext) {
    let readme = "# Ferrite\n\nA *fast* [file manager](https://example.com) for $0.\n\n\
        - one\n- two\n  1. nested\n\n| name | size |\n|------|------|\n| a.txt | 3 |\n\n\
        Use `a < b` inline.\n\n```rust\nif a < b { x > y }\n```\n\n![logo](logo.png)\n";
    let (_, view) = open(cx, "README.md", readme);
    assert!(!has_editor(cx, &view));
    let rendered = markdown_text(cx, &view);
    for part in [
        "Ferrite",
        "file manager",
        "for $0.",
        "nested",
        "a.txt",
        "a < b",
        "if a < b { x > y }",
        "Image: logo",
    ] {
        assert!(rendered.contains(part), "{part:?} in {rendered:?}");
    }
    assert!(!rendered.contains('\\'), "{rendered:?}");
}

#[gpui_kit::test]
fn html_and_math_render_as_literal_text(cx: &mut TestAppContext) {
    let source = "<div>\n<b>bold</b>\n</div>\n\nCosts $5 and $$x$$, or <i>this</i>.\n";
    let (_, view) = open(cx, "html.md", source);
    let rendered = markdown_text(cx, &view);
    assert!(rendered.contains("<b>bold</b>"), "{rendered}");
    assert!(
        rendered.contains("Costs $5 and $$x$$, or <i>this</i>."),
        "{rendered}"
    );
}

fn opens_as_text_with(cx: &mut TestAppContext, name: &str, source: String, note: &str) {
    let (window, view) = open(cx, name, &source);
    assert!(has_editor(cx, &view), "{name} opens in the text view");
    assert_eq!(text_of(cx, window, "viewer-message"), note);
}

#[gpui_kit::test]
fn markdown_over_1_mib_opens_as_text_with_a_note(cx: &mut TestAppContext) {
    let source = "a line\n".repeat(160_000);
    opens_as_text_with(cx, "big.md", source, "Large Markdown: shown as text.");
}

#[gpui_kit::test]
fn markdown_with_too_many_blocks_opens_as_text_with_a_note(cx: &mut TestAppContext) {
    let source = "- a\n".repeat(200_001);
    let note = "Markdown with over 200,000 blocks: shown as text.";
    opens_as_text_with(cx, "many.md", source, note);
}

#[gpui_kit::test]
fn a_100_000_block_file_lays_out_only_the_visible_blocks(cx: &mut TestAppContext) {
    let source = "p\n\n".repeat(100_000);
    let (window, view) = open(cx, "long.md", &source);
    assert!(!has_editor(cx, &view));
    let drawn = |cx: &mut TestAppContext| with_body(cx, &view, MarkdownBody::drawn);
    let before = drawn(cx);
    assert!(before > 0, "some blocks are drawn");
    redraw(cx, window);
    let one_frame = drawn(cx) - before;
    assert!(
        (1..200).contains(&one_frame),
        "{one_frame} blocks in a frame"
    );
    press(cx, window, &view, "ctrl-end");
    let before = drawn(cx);
    redraw(cx, window);
    assert!(drawn(cx) - before < 200, "the end draws only what shows");
}

#[gpui_kit::test]
fn tab_picks_links_in_order_and_enter_opens_them(cx: &mut TestAppContext) {
    let source = "[one](https://example.com/1) and [two](javascript:alert(1))\n";
    let (window, view) = open(cx, "links.md", source);
    press(cx, window, &view, "tab");
    assert_eq!(
        text_of(cx, window, "markdown-link"),
        "Enter opens https://example.com/1"
    );
    press(cx, window, &view, "enter");
    assert_eq!(
        text_of(cx, window, "markdown-status"),
        "Opened https://example.com/1"
    );
    press(cx, window, &view, "tab");
    press(cx, window, &view, "enter");
    let status = text_of(cx, window, "markdown-status");
    assert!(status.starts_with("Not opened: javascript:"), "{status}");
    press(cx, window, &view, "shift-tab");
    assert_eq!(
        text_of(cx, window, "markdown-link"),
        "Enter opens https://example.com/1"
    );
}

#[gpui_kit::test]
fn a_long_code_line_scrolls_sideways_inside_its_block(cx: &mut TestAppContext) {
    let source = format!("```\n{}\n```\n", "let wide = 1; ".repeat(60));
    let (window, view) = open(cx, "wide.md", &source);
    let bounds = |cx: &mut TestAppContext, id: (&'static str, usize)| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find(id).bounds()
        })
        .expect("window")
    };
    let frame = bounds(cx, ("md-code", 0));
    let line = bounds(cx, ("md-code-text", 0));
    assert!(line.size.width > frame.size.width, "{line:?} in {frame:?}");
    cx.update_window(window, |_, window, cx| {
        window.scroll(
            ("md-code", 0usize),
            ScrollDelta::Pixels(point(px(-200.), px(0.))),
            cx,
        );
    })
    .expect("window");
    redraw(cx, window);
    let moved = bounds(cx, ("md-code-text", 0));
    assert!(moved.origin.x < line.origin.x, "{moved:?} after {line:?}");
    assert!(!has_editor(cx, &view));
}

#[gpui_kit::test]
fn too_many_table_cells_show_tables_as_text_with_a_status_note(cx: &mut TestAppContext) {
    let one = "|a".repeat(362) + "|\n" + &"|-".repeat(362) + "|\n" + &"x\n".repeat(724) + "\n";
    let (window, view) = open(cx, "tables.md", &one.repeat(4));
    assert!(!has_editor(cx, &view));
    assert_eq!(
        text_of(cx, window, "markdown-status"),
        "Tables shown as text: too many cells"
    );
}

#[gpui_kit::test]
fn long_link_targets_are_cut_to_200_characters(cx: &mut TestAppContext) {
    let target = format!("https://example.com/{}", "a".repeat(300));
    let (window, view) = open(cx, "long.md", &format!("[x]({target})\n"));
    press(cx, window, &view, "tab");
    let detail = text_of(cx, window, "markdown-link");
    assert_eq!(detail.chars().count(), "Enter opens ".len() + 201);
    assert!(detail.ends_with('…'), "{detail}");
    press(cx, window, &view, "enter");
    let status = text_of(cx, window, "markdown-status");
    assert_eq!(status, format!("Opened {}…", &target[..200]));
}

fn on_a_small_stack(stack: usize, test: fn(&mut TestAppContext)) {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            gpui_kit::run_test_once(
                0,
                Box::new(move |dispatcher| {
                    let mut cx = TestAppContext::build(dispatcher, Some("small_stack"));
                    test(&mut cx);
                    cx.run_until_parked();
                    cx.update(|cx| cx.quit());
                    cx.run_until_parked();
                }),
            );
        })
        .expect("the test thread starts")
        .join()
        .expect("the render finishes");
}

const RENDER_STACK: usize = if cfg!(debug_assertions) {
    6 * 1024 * 1024
} else {
    1024 * 1024
};

fn tags(name: &str, levels: usize) -> String {
    format!("<{name}>").repeat(levels) + "x"
}

fn probes() -> Vec<String> {
    let n = 5_000;
    vec![
        "*".repeat(50_000) + "x" + &"*".repeat(50_000) + "\n",
        "*a ".repeat(50_000) + "x",
        (0..1_000)
            .map(|depth| format!("{}- x\n", "  ".repeat(depth)))
            .collect(),
        (0..1_000)
            .map(|depth| format!("> {}- x\n", "  ".repeat(depth)))
            .collect(),
        ">".repeat(100_000) + " x\n",
        "<b></i>".repeat(n) + "x" + &"</b>".repeat(n),
        "a $".to_owned() + &"<b>".repeat(n) + "x" + &"</b>".repeat(n) + "$",
        tags("div", 20_000),
        tags("details", 20_000),
        "|a".repeat(100) + "|\n" + &"|-".repeat(100) + "|\n" + &"x\n".repeat(1_000),
    ]
}

#[test]
fn the_reviewer_probes_render_on_a_small_stack() {
    on_a_small_stack(RENDER_STACK, |cx| {
        for (at, probe) in probes().into_iter().enumerate() {
            let name = format!("probe{at}.md");
            let (_, view) = open(cx, &name, &probe);
            assert!(!has_editor(cx, &view), "{name} renders as markdown");
            let rendered = markdown_text(cx, &view);
            assert!(rendered.contains('x'), "{name} shows its text");
            assert!(!rendered.contains('\\'), "{name} shows no escapes");
        }
    });
}
