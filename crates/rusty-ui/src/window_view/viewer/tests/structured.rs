use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_fs::FakeFs;

use super::{Log, Setup, editor_text, has_editor, hooks, label, press_only, redraw, text_of};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::clipboard::FakeClipboard;
use crate::window_view::tests::{press, settle, type_in, viewer_open};
use crate::window_view::viewer::{FileHooks, Kinds, PagedKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum Read {
    Bytes(u64),
    Range(u64, usize),
}

pub(in crate::window_view::viewer) type Reads = Arc<Mutex<Vec<Read>>>;

fn reads(log: &Reads) -> Vec<Read> {
    log.lock().expect("reads").clone()
}

fn logged(setup: Setup, log: &Reads) -> FileHooks {
    let data = Arc::new(setup.bytes.clone());
    let files = hooks(setup, &Log::default());
    let (whole, part) = (data.clone(), data);
    let (bytes_log, range_log) = (log.clone(), log.clone());
    FileHooks {
        bytes: Arc::new(move |_, limit| {
            bytes_log.lock().expect("reads").push(Read::Bytes(limit));
            Ok(whole.as_ref().clone())
        }),
        range: Arc::new(move |_, offset, len| {
            range_log
                .lock()
                .expect("reads")
                .push(Read::Range(offset, len));
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(part.len());
            Ok(part[start..(start + len).min(part.len())].to_vec())
        }),
        ..files
    }
}

pub(in crate::window_view::viewer) fn fake_with(name: &str, len: u64) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{name}"), len);
    fs.add_folder("dest");
    Arc::new(fs)
}

pub(in crate::window_view::viewer) fn open_named(
    cx: &mut TestAppContext,
    name: &str,
    setup: Setup,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Reads) {
    let log = Reads::default();
    let len = setup.len.unwrap_or(setup.bytes.len() as u64);
    let files = logged(setup, &log);
    let (window, view) = super::window_with(cx, fake_with(name, len), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    assert!(reads(&log).is_empty(), "nothing is read on the UI thread");
    settle(cx, &view);
    redraw(cx, window);
    (window, view, log)
}

fn wait_for_filter(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
}

const STORE: &[u8] =
    br#"{"store": {"book": [{"title": "Sayings"}, {"title": "Sword"}]}, "name": "shop"}"#;

#[gpui_kit::test]
fn f3_on_json_shows_a_tree_right_expands_and_the_header_shows_the_path(cx: &mut TestAppContext) {
    let (window, view, log) = open_named(cx, "shop.json", Setup::new(STORE.to_vec()));
    assert_eq!(reads(&log), [Read::Bytes(rusty_structured::TREE_LIMIT)]);
    assert_eq!(text_of(cx, window, "tree-path"), "$");
    assert!(label(cx, window, ("tree-row", 1)).starts_with("store"));
    assert!(label(cx, window, ("tree-row", 2)).starts_with("name"));
    press(cx, window, &view, "down");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store");
    press(cx, window, &view, "right");
    assert!(label(cx, window, ("tree-row", 2)).starts_with("book"));
    press(cx, window, &view, "right");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store.book");
    press(cx, window, &view, "ctrl-shift-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("$.store.book"));
    press(cx, window, &view, "left");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store");
}

#[gpui_kit::test]
fn star_minus_and_typing_filter_the_tree_and_escape_steps_back(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "shop.json", Setup::new(STORE.to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "*");
    assert!(label(cx, window, ("tree-row", 6)).starts_with("title"));
    press(cx, window, &view, "-");
    assert!(label(cx, window, ("tree-row", 2)).starts_with("name"));
    for key in ["t", "i", "t"] {
        press(cx, window, &view, key);
    }
    wait_for_filter(cx, window, &view);
    assert_eq!(text_of(cx, window, "filter"), "Filter tit, 2 items");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store.book[0].title");
    press(cx, window, &view, "n");
    wait_for_filter(cx, window, &view);
    assert_eq!(text_of(cx, window, "filter"), "Filter titn, 0 items");
    press(cx, window, &view, "backspace");
    wait_for_filter(cx, window, &view);
    press(cx, window, &view, "escape");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store.book[0].title");
    assert!(super::super::super::tests::viewer_open(cx, &view));
    press(cx, window, &view, "escape");
    assert!(!super::super::super::tests::viewer_open(cx, &view));
}

#[gpui_kit::test]
fn a_parse_error_shows_line_and_column_and_1_opens_the_text(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "broken.json", Setup::new(b"{\n  \"a\": }\n".to_vec()));
    let error = text_of(cx, window, "tree-error");
    assert!(error.contains("line 2, column 8"), "{error}");
    press(cx, window, &view, "1");
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "{\n  \"a\": }\n");
}

#[gpui_kit::test]
fn json_over_64_mb_opens_as_paged_text_and_the_header_says_why(cx: &mut TestAppContext) {
    let setup = Setup {
        len: Some(rusty_structured::TREE_LIMIT + 1),
        ..Setup::new(STORE.to_vec())
    };
    let (window, view, log) = open_named(cx, "shop.json", setup);
    assert!(reads(&log).is_empty());
    assert_eq!(super::kind(cx, &view), Some(PagedKind::Text));
    let status = text_of(cx, window, "paged-status");
    assert!(status.contains("64 MB"), "{status}");
}

pub(in crate::window_view::viewer) fn big_xml() -> Vec<u8> {
    let mut text = String::from("<?xml version=\"1.0\"?>\n<AUTOSAR>\n");
    for package in 0..3 {
        text.push_str(&format!("<AR-PACKAGE N=\"{package}\">\n"));
        for element in 0..40 {
            text.push_str(&format!(
                "  <ELEMENT><SHORT-NAME>S{element}</SHORT-NAME></ELEMENT>\n"
            ));
        }
        text.push_str("</AR-PACKAGE>\n");
    }
    text.push_str("</AUTOSAR>\n");
    text.into_bytes()
}

#[gpui_kit::test]
fn xml_over_16_mb_is_indexed_through_ranged_reads_and_expands_on_demand(cx: &mut TestAppContext) {
    let bytes = big_xml();
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(bytes)
    };
    let (window, view, log) = open_named(cx, "big.arxml", setup);
    let seen = reads(&log);
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|read| matches!(read, Read::Range(..))),
        "{seen:?}"
    );
    assert_eq!(text_of(cx, window, "tree-status"), "244 elements");
    assert!(label(cx, window, ("tree-row", 2)).starts_with("AR-PACKAGE"));
    log.lock().expect("reads").clear();
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "right");
    assert!(!reads(&log).is_empty());
    assert!(label(cx, window, ("tree-row", 3)).starts_with("ELEMENT"));
    press(cx, window, &view, "right");
    assert_eq!(
        text_of(cx, window, "tree-path"),
        "/AUTOSAR/AR-PACKAGE[2]/ELEMENT[1]"
    );
}

const FRUIT: &[u8] = b"name,qty\nPear,10\nApple,9\nFig,100\n";

#[gpui_kit::test]
fn f3_on_csv_shows_the_header_and_cells_and_a_header_click_sorts(cx: &mut TestAppContext) {
    let (window, view, log) = open_named(cx, "fruit.csv", Setup::new(FRUIT.to_vec()));
    assert_eq!(reads(&log), [Read::Bytes(rusty_structured::SMALL_LIMIT)]);
    assert_eq!(label(cx, window, ("table-head", 0)), "name");
    assert_eq!(label(cx, window, ("table-head", 1)), "qty");
    assert_eq!(label(cx, window, ("table-row", 0)), "1: Pear, 10");
    cx.update_window(window, |_, window, cx| {
        window.click(("table-head", 1usize), cx)
    })
    .expect("window");
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(label(cx, window, ("table-row", 0)), "2: Apple, 9");
    assert_eq!(label(cx, window, ("table-row", 2)), "3: Fig, 100");
    press(cx, window, &view, "right");
    press(cx, window, &view, "ctrl-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("9"));
    press(cx, window, &view, "ctrl-shift-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("Apple\t9"));
    press(cx, window, &view, "ctrl-f3");
    assert_eq!(label(cx, window, ("table-row", 0)), "3: Fig, 100");
}

#[gpui_kit::test]
fn csv_over_16_mb_reads_rows_on_demand_and_says_sort_is_off(cx: &mut TestAppContext) {
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(FRUIT.to_vec())
    };
    let (window, view, log) = open_named(cx, "fruit.csv", setup);
    let seen = reads(&log);
    assert!(
        seen.iter().all(|read| matches!(read, Read::Range(..))),
        "{seen:?}"
    );
    assert_eq!(label(cx, window, ("table-head", 0)), "name");
    assert_eq!(label(cx, window, ("table-row", 1)), "2: Apple, 9");
    cx.update_window(window, |_, window, cx| {
        window.click(("table-head", 1usize), cx)
    })
    .expect("window");
    settle(cx, &view);
    assert!(text_of(cx, window, "table-status").contains("Sorting is off"));
    assert_eq!(label(cx, window, ("table-row", 0)), "1: Pear, 10");
}

#[gpui_kit::test]
fn typing_a_filter_waits_before_filtering_the_tree(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "shop.json", Setup::new(STORE.to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "*");
    press(cx, window, &view, "t");
    assert_eq!(text_of(cx, window, "filter"), "Filter t, 0 items");
    wait_for_filter(cx, window, &view);
    assert_eq!(text_of(cx, window, "filter"), "Filter t, 3 items");
}

#[gpui_kit::test]
fn escape_before_a_pending_filter_applies_keeps_the_cursor(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "shop.json", Setup::new(STORE.to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "x");
    press(cx, window, &view, "escape");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store");
    press(cx, window, &view, "*");
    for key in ["t", "i", "t"] {
        press(cx, window, &view, key);
    }
    wait_for_filter(cx, window, &view);
    press(cx, window, &view, "n");
    wait_for_filter(cx, window, &view);
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "escape");
    assert_eq!(text_of(cx, window, "tree-path"), "$.store.book[0].title");
}

#[gpui_kit::test]
fn a_csv_wider_than_4096_columns_says_how_many_are_shown(cx: &mut TestAppContext) {
    let line = vec!["1"; 5000].join(",");
    let source = format!("{line}\n{line}\n");
    let (window, _, _) = open_named(cx, "wide.csv", Setup::new(source.into_bytes()));
    let status = text_of(cx, window, "table-status");
    assert!(status.contains("4,096 of 5,000 columns"), "{status}");
}

#[gpui_kit::test]
fn indexed_rows_wider_than_the_first_record_widen_the_header(cx: &mut TestAppContext) {
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(b"name,qty\nPear,10,ripe,green,local\n".to_vec())
    };
    let (window, _, _) = open_named(cx, "wide.csv", setup);
    assert_eq!(label(cx, window, ("table-head", 4)), "E");
    assert_eq!(
        label(cx, window, ("table-row", 0)),
        "1: Pear, 10, ripe, green, local"
    );
}

#[gpui_kit::test]
fn a_short_row_fetch_is_not_asked_again_on_every_frame(cx: &mut TestAppContext) {
    let mut bytes = b"name,qty\n".to_vec();
    bytes.resize(bytes.len() + 9 * 1024 * 1024, b'x');
    bytes.extend_from_slice(b",1\nFig,2\n");
    let setup = Setup {
        len: Some(rusty_structured::SMALL_LIMIT + 1),
        ..Setup::new(bytes)
    };
    let (window, view, log) = open_named(cx, "huge-row.csv", setup);
    assert_eq!(label(cx, window, ("table-row", 1)), "2: Fig, 2");
    log.lock().expect("reads").clear();
    for _ in 0..3 {
        redraw(cx, window);
        settle(cx, &view);
    }
    assert!(reads(&log).is_empty(), "{:?}", reads(&log));
}

#[gpui_kit::test]
fn ctrl_c_in_the_table_viewer_copies_the_cell_not_the_file(cx: &mut TestAppContext) {
    let files = Arc::new(FakeClipboard::default());
    let shared: Arc<dyn crate::window_view::FileClipboardService> = files.clone();
    let options = crate::window_view::StartOptions {
        file_clipboard: Some(shared),
        ..super::options_with(
            fake_with("fruit.csv", FRUIT.len() as u64),
            hooks(Setup::new(FRUIT.to_vec()), &Log::default()),
            Kinds::builtin(),
        )
    };
    let (window, view) = crate::window_view::tests::open_options(cx, options);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    press(cx, window, &view, "right");
    press(cx, window, &view, "ctrl-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("10"));
    assert!(files.writes.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn ctrl_f_in_the_table_counts_matching_cells_and_enter_steps_through_them(cx: &mut TestAppContext) {
    let csv = b"name,qty\nPear,10\nApple,9\nFig,100\nPeach,1\n";
    let (window, view, _) = open_named(cx, "fruit.csv", Setup::new(csv.to_vec()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "p");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 3");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "find-count"), "2 of 3");
    assert_eq!(text_of(cx, window, "table-cell"), "Row 2 · name");
    press(cx, window, &view, "shift-enter");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 3");
    press(cx, window, &view, "escape");
    assert_eq!(text_of(cx, window, "find-count"), "");
    assert!(viewer_open(cx, &view));
}
