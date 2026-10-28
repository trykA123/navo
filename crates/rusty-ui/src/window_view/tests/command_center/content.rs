use std::sync::mpsc;

use rusty_core::ProviderPath;
use rusty_core::find::{FileHits, LineHit};
use rusty_fs::find::{FindProgress, FoundBatch, FoundFile};
use rusty_fs::{Attributes, CancelToken, EntryKind, FsEntry};

use super::*;
use crate::command_center::ContentSource;

fn batch(relative: &str) -> FoundBatch {
    FoundBatch {
        files: vec![FoundFile {
            relative: ProviderPath::from_text(relative),
            entry: FsEntry {
                name: ProviderPath::from_text("a.txt"),
                kind: EntryKind::File,
                size: 10,
                modified_ms: None,
                attributes: Attributes::default(),
            },
            hits: FileHits {
                hits: vec![LineHit {
                    line: 3,
                    column: 0,
                    text: "third needle".into(),
                }],
                total: 1,
                skipped: None,
            },
        }],
        progress: FindProgress::default(),
    }
}

fn content_rig(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.view.update(cx, |view, _| {
        view.command_center.content_source = ContentSource::with_walk(Arc::new(|_, _, _, sink| {
            let _ = sink(batch("a.txt"));
            Ok(FindProgress::default())
        }));
    });
    rig.press(cx, "alt-f7");
    rig
}

#[gpui_kit::test]
fn content_tokens_show_chips_and_flag_keys_toggle_them(cx: &mut TestAppContext) {
    let rig = content_rig(cx);
    rig.type_query(cx, "needle ext:rs size:oops");
    let labels = |cx: &mut TestAppContext| {
        rig.read(cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("command-center-content-chip", index)))
                .map(|chip| chip.label().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(labels(cx), ["ext:rs", "size:oops"]);
    rig.press(cx, "alt-c");
    rig.press(cx, "alt-r");
    assert_eq!(labels(cx), ["ext:rs", "size:oops", "Case", "Regex"]);
    rig.press(cx, "alt-c");
    rig.press(cx, "alt-r");
    assert_eq!(labels(cx), ["ext:rs", "size:oops"]);
}

#[gpui_kit::test]
fn ctrl_enter_starts_a_results_tab_with_the_content_filters(cx: &mut TestAppContext) {
    let rig = content_rig(cx);
    rig.type_query(cx, "needle ext:rs");
    rig.press(cx, "alt-c");
    let tabs = rig.tab_count(cx);
    rig.press(cx, "ctrl-enter");
    assert_eq!(rig.tab_count(cx), tabs + 1);
    rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        assert_eq!(tab.location().provider().as_str(), "find");
        let query = &view.browser.find_results(tab.id()).unwrap().query;
        assert!(query.case);
        assert_eq!(query.ext, ["rs"]);
    });
}

#[gpui_kit::test]
fn a_new_key_cancels_the_previous_preview(cx: &mut TestAppContext) {
    let rig = content_rig(cx);
    let (send, receive) = mpsc::channel::<CancelToken>();
    rig.view.update(cx, |view, _| {
        view.command_center.content_source =
            ContentSource::with_walk(Arc::new(move |_, _, cancel, sink| {
                let _ = sink(batch("a.txt"));
                send.send(cancel.clone()).unwrap();
                if cancel.wait_cancelled(Duration::from_secs(5)) {
                    return Ok(FindProgress::default());
                }
                Ok(FindProgress::default())
            }));
    });
    rig.type_query(cx, "needle");
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    let cancel = receive
        .recv_timeout(Duration::from_secs(2))
        .expect("search started");
    cx.executor().advance_clock(Duration::from_millis(16));
    settle(cx, &rig.view);
    assert_eq!(rig.rows(cx), ["third needle · a.txt:3"]);
    rig.type_query(cx, "x");
    assert!(cancel.is_cancelled());
    assert!(rig.rows(cx).is_empty());
}

#[gpui_kit::test]
async fn an_invalid_content_token_explains_its_error_on_hover(cx: &mut TestAppContext) {
    use gpui_kit::test::TestAppContextExt;
    let rig = content_rig(cx);
    rig.type_query(cx, "needle size:oops");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("command-center-content-chip", 0usize), cx);
    })
    .expect("window");
    cx.wait_for(rig.window, Duration::from_secs(2), |window, _| {
        window
            .try_find("tooltip")
            .is_some_and(|tip| tip.label() == Some("size: needs a number, like <50MB"))
    })
    .await;
}

#[gpui_kit::test]
fn content_preview_debounces_and_stops_after_two_seconds(cx: &mut TestAppContext) {
    let rig = content_rig(cx);
    let (send, receive) = mpsc::channel::<CancelToken>();
    rig.view.update(cx, |view, _| {
        view.command_center.content_source =
            ContentSource::with_walk(Arc::new(move |_, _, cancel, _| {
                send.send(cancel.clone()).unwrap();
                cancel.wait_cancelled(Duration::from_secs(5));
                Ok(FindProgress::default())
            }));
    });
    rig.type_query(cx, "needle");
    cx.executor().advance_clock(Duration::from_millis(199));
    cx.run_until_parked();
    assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    let cancel = receive
        .recv_timeout(Duration::from_secs(2))
        .expect("search started");
    assert!(!cancel.is_cancelled());
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(cancel.is_cancelled());
}
