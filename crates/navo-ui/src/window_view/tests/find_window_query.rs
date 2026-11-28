use std::cell::RefCell;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, Role, TestAppContext, px, size,
};
use navo_core::Location;
use navo_core::find::FindQuery;
use navo_fs::find::FindProgress;

use super::super::NavoWindow;
use super::{fake, open_options, options_for, settle, two_panes};
use crate::command_center::ContentSource;
use crate::find_results::run::{FindRun, RunEvent};
use crate::test_pointer::click;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    walk_rx: mpsc::Receiver<FindQuery>,
    walk_calls: RefCell<Vec<FindQuery>>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    settle(cx, &view);

    let (tx, walk_rx) = mpsc::sync_channel(32);
    view.update(cx, |view, _| {
        view.command_center.content_source =
            ContentSource::with_walk(Arc::new(move |_, query, _, _| {
                let _ = tx.send(query.clone());
                Ok(FindProgress::default())
            }));
    });

    Rig {
        window,
        view,
        walk_rx,
        walk_calls: RefCell::new(Vec::new()),
    }
}

impl Rig {
    fn open_find(&self, cx: &mut TestAppContext) -> AnyWindowHandle {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| view.open_find(window, cx));
        })
        .expect("navo window");
        cx.run_until_parked();
        let handle = self
            .view
            .read_with(cx, |view, _| view.find.window_handle())
            .expect("find window handle");
        cx.simulate_window_resize(handle, size(px(640.), px(420.)));
        cx.run_until_parked();
        handle
    }

    fn add_tab(&self, cx: &mut TestAppContext, text: &str) -> u64 {
        self.view.update(cx, |view, cx| {
            view.find.book().update(cx, |book, cx| {
                let id = book.add(text, Location::new("/tmp/find").expect("test location"));
                cx.notify();
                id
            })
        })
    }

    fn active_text(&self, cx: &TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, cx| {
            view.find
                .book()
                .read(cx)
                .active()
                .map(|tab| tab.text.clone())
        })
    }

    fn active_query(&self, cx: &TestAppContext) -> Option<FindQuery> {
        self.view.read_with(cx, |view, cx| {
            view.find
                .book()
                .read(cx)
                .active()
                .map(|tab| tab.query.clone())
        })
    }

    fn active_running(&self, cx: &TestAppContext) -> bool {
        self.view.read_with(cx, |view, cx| {
            view.find
                .book()
                .read(cx)
                .active()
                .is_some_and(|tab| tab.is_running())
        })
    }

    fn press_in(&self, cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("find window");
        cx.run_until_parked();
    }

    fn has_walks(&self) -> bool {
        let mut calls = self.walk_calls.borrow_mut();
        while let Ok(query) = self.walk_rx.try_recv() {
            calls.push(query);
        }
        !calls.is_empty()
    }

    fn wait_for_walks(&self, expected: usize) -> Vec<FindQuery> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut calls = self.walk_calls.borrow_mut();
        while calls.len() < expected {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            match self.walk_rx.recv_timeout(left) {
                Ok(query) => calls.push(query),
                Err(_) => break,
            }
        }
        calls.clone()
    }
}

#[gpui_kit::test]
fn query_row_renders_at_36px_with_token_chips_and_bad_token_message(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "needle ext:rs size:bad");
    let find = rig.open_find(cx);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let row = window.find("find-query-row");
        assert_eq!(row.bounds().size.height, px(36.));

        let first = window.find(("find-query-chip", 0usize));
        assert!(first.visible());
        assert_eq!(first.label(), Some("needle"));
        assert_eq!(first.description(), None);

        let second = window.find(("find-query-chip", 1usize));
        assert!(second.visible());
        assert_eq!(second.label(), Some("ext:rs"));
        assert_eq!(second.description(), None);

        let bad = window.find(("find-query-chip", 2usize));
        assert!(bad.visible());
        assert_eq!(bad.label(), Some("size:bad"));
        let expected_error = navo_l10n::t!("find-token-error-size");
        assert_eq!(bad.description(), Some(expected_error.as_str()));
    })
    .expect("find window");
}

#[gpui_kit::test]
fn clicking_chip_remove_reprints_with_to_text_and_restarts_run(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "needle ext:rs size:>10k");
    let find = rig.open_find(cx);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click(("find-query-chip-remove", 1usize), cx);
    })
    .expect("find window");
    cx.run_until_parked();

    assert_eq!(rig.active_text(cx).as_deref(), Some("needle size:>10240"));
    let query = rig.active_query(cx).expect("active query");
    assert_eq!(query.text, "needle");
    assert!(query.ext.is_empty());
    assert!(query.size.is_some());

    let calls = rig.wait_for_walks(1);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].text, "needle");
    assert!(calls[0].ext.is_empty());
    assert!(calls[0].size.is_some());
}

#[gpui_kit::test]
fn overflow_badge_renders_when_many_tokens_exceed_row_width(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(
        cx,
        "alpha beta ext:rs,toml size:>1k mod:7d depth:2 word:on case:on docs:on arc:on",
    );
    let find = rig.open_find(cx);
    cx.simulate_window_resize(find, size(px(420.), px(300.)));
    cx.run_until_parked();

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let overflow = window.find("find-query-overflow");
        assert!(overflow.visible());
        let shown_chips = (0..10usize)
            .filter(|&idx| window.try_find(("find-query-chip", idx)).is_some())
            .count();
        assert!(shown_chips < 10, "expected overflow to hide some chips");
        let hidden = 10 - shown_chips;
        let expected = format!("+{hidden}");
        assert_eq!(overflow.label(), Some(expected.as_str()));
    })
    .expect("find window");
}

#[gpui_kit::test]
fn case_and_regex_toggles_update_query_and_rerun_via_click_and_keys(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "needle");
    let find = rig.open_find(cx);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query-case").role(), Some(Role::Button));
        assert_eq!(window.find("find-query-regex").role(), Some(Role::Button));
        window.click("find-query-case", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    assert!(rig.active_query(cx).expect("query").case);
    assert_eq!(rig.wait_for_walks(1).last().map(|q| q.case), Some(true));

    rig.press_in(cx, find, "alt-c");
    assert!(!rig.active_query(cx).expect("query").case);
    assert_eq!(rig.wait_for_walks(2).last().map(|q| q.case), Some(false));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("find-query-regex", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    assert!(rig.active_query(cx).expect("query").regex);
    assert_eq!(rig.wait_for_walks(3).last().map(|q| q.regex), Some(true));

    rig.press_in(cx, find, "alt-r");
    assert!(!rig.active_query(cx).expect("query").regex);
    assert_eq!(rig.wait_for_walks(4).last().map(|q| q.regex), Some(false));
}

#[gpui_kit::test]
fn stop_button_stops_running_search_and_run_again_restarts_idle_search(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let id = rig.add_tab(cx, "needle");
    let (_send, receive) = mpsc::sync_channel::<RunEvent>(4);
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, cx| {
            book.tab_mut(id).expect("tab").run = Some(FindRun::from_receiver(receive));
            cx.notify();
        });
    });
    let find = rig.open_find(cx);
    assert!(rig.active_running(cx));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("find-query-stop").visible());
        assert!(window.try_find("find-query-run").is_none());
        window.click("find-query-stop", cx);
    })
    .expect("find window");
    cx.run_until_parked();
    assert!(!rig.active_running(cx));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("find-query-run").visible());
        assert!(window.try_find("find-query-stop").is_none());
        window.click("find-query-run", cx);
    })
    .expect("find window");
    cx.run_until_parked();
    assert_eq!(rig.wait_for_walks(1).len(), 1);

    rig.press_in(cx, find, "ctrl-r");
    assert_eq!(rig.wait_for_walks(2).len(), 2);
}

#[gpui_kit::test]
fn ctrl_l_enters_inline_edit_escape_cancels_and_enter_commits_and_reruns(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "alpha ext:rs");
    let find = rig.open_find(cx);

    // Ctrl+L enters inline edit mode (FindResults > Input).
    rig.press_in(cx, find, "ctrl-l");
    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("find-query-input").visible());
        window.input("beta", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    // Escape cancels edit without changing tab query or closing/leaving the Find window.
    rig.press_in(cx, find, "escape");
    assert_eq!(rig.active_text(cx).as_deref(), Some("alpha ext:rs"));
    assert!(!rig.has_walks());
    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("find-query-input").is_none());
        assert!(window.is_window_active());
    })
    .expect("find window");

    // Clicking the query strip also enters inline edit mode; Enter commits & re-runs.
    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("find-query-strip", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("find-query-input").visible());
        window.input("gamma ext:md", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    rig.press_in(cx, find, "enter");
    assert_eq!(rig.active_text(cx).as_deref(), Some("gamma ext:md"));
    let query = rig.active_query(cx).expect("query");
    assert_eq!(query.text, "gamma");
    assert_eq!(query.ext, vec!["md".to_owned()]);
    let calls = rig.wait_for_walks(1);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].text, "gamma");
    assert_eq!(calls[0].ext, vec!["md".to_owned()]);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("find-query-input").is_none());
    })
    .expect("find window");
}

#[gpui_kit::test]
fn switching_tab_clears_inline_edit_state(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, cx| {
            book.select(first);
            cx.notify();
        });
    });
    let find = rig.open_find(cx);

    rig.press_in(cx, find, "ctrl-l");
    let tab_at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("find-query-input").visible());
            window.find(("find-tab", second)).bounds().center()
        })
        .expect("find window");

    click(cx, find, (tab_at, Modifiers::default()));
    cx.run_until_parked();

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("find-query-input").is_none());
        assert_eq!(
            window.find(("find-query-chip", 0usize)).label(),
            Some("beta")
        );
    })
    .expect("find window");
}
