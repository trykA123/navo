use std::sync::{Arc, Mutex};

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;

use super::{SLOW, key};
use crate::window_view::tests::{settle, undo};

fn slow_hooks(calls: Arc<Mutex<usize>>) -> crate::browser::RenameHooks {
    crate::browser::RenameHooks {
        last: Arc::new(move || {
            std::thread::sleep(SLOW);
            let mut calls = calls.lock().unwrap();
            *calls += 1;
            Err(format!("query {}", *calls))
        }),
        ..undo::Journal::default().hooks()
    }
}

#[gpui_kit::test]
fn undo_queries_return_at_once_and_apply_results_in_order(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(0));
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(calls.clone()),
    );
    *calls.lock().unwrap() = 0;
    key(cx, rig.window, "ctrl-z");
    key(cx, rig.window, "ctrl-z");
    assert_eq!(*calls.lock().unwrap(), 0);
    settle(cx, &rig.view);
    assert_eq!(*calls.lock().unwrap(), 2);
    assert!(rig.notice(cx).is_some_and(|text| text.contains("query 2")));
}

#[gpui_kit::test]
fn closing_a_window_with_an_undo_query_pending_discards_the_reply(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(0));
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(calls.clone()),
    );
    *calls.lock().unwrap() = 0;
    key(cx, rig.window, "ctrl-z");
    assert!(rig.view.read_with(cx, |view, _| view.lane.is_pending()));
    cx.update_window(rig.window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(cx.update_window(rig.window, |_, _, _| ()).is_err());
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[gpui_kit::test]
fn superseded_undo_reply_is_silent(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(0));
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(calls.clone()),
    );
    *calls.lock().unwrap() = 0;
    key(cx, rig.window, "ctrl-z");
    rig.view.update(cx, |view, _| {
        view.lane.push(Box::new(|| {
            Box::new(|view, _, _| {
                assert!(
                    view.status_notice().is_none(),
                    "a superseded query must stay silent"
                );
            })
        }));
    });
    key(cx, rig.window, "ctrl-z");
    settle(cx, &rig.view);
    assert!(rig.notice(cx).is_some_and(|text| text.contains("query 2")));
}

#[gpui_kit::test]
fn undo_reply_after_a_prompt_opened_and_closed_is_silent(cx: &mut TestAppContext) {
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(Arc::default()),
    );
    key(cx, rig.window, "ctrl-z");
    key(cx, rig.window, "f7");
    key(cx, rig.window, "escape");
    settle(cx, &rig.view);
    assert!(rig.notice(cx).is_none());
}

#[gpui_kit::test]
fn workspace_undo_queries_the_journal_off_the_ui_thread(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(0));
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(calls.clone()),
    );
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.apply(
                &rusty_core::Command::WorkspaceNew { name: None },
                window,
                cx,
            );
            view.delete_workspace(2, window, cx);
            view.forget_clipboard_notice();
        });
    })
    .unwrap();
    *calls.lock().unwrap() = 0;
    key(cx, rig.window, "ctrl-z");
    assert_eq!(*calls.lock().unwrap(), 0);
    settle(cx, &rig.view);
    rig.view.read_with(cx, |view, _| {
        let state = view.browser.state();
        assert_eq!(
            state
                .window(state.focused_window())
                .unwrap()
                .workspaces()
                .len(),
            2
        );
    });
}

#[gpui_kit::test]
fn undo_reply_after_a_job_started_is_silent(cx: &mut TestAppContext) {
    let fs = undo::files();
    let rig = undo::open_with(
        cx,
        fs.clone(),
        crate::browser::Operations::unlogged(),
        slow_hooks(Arc::default()),
    );
    key(cx, rig.window, "ctrl-z");
    rig.view.update(cx, |view, _| {
        view.browser.rename("a.txt", "kept.txt").unwrap()
    });
    settle(cx, &rig.view);
    assert!(rig.notice(cx).is_none());
    assert!(rusty_fs::Provider::metadata(fs.as_ref(), &fs.location("home/kept.txt")).is_ok());
}

#[gpui_kit::test]
fn undo_reply_after_a_job_finished_is_silent(cx: &mut TestAppContext) {
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        slow_hooks(Arc::default()),
    );
    rig.view.update(cx, |view, _| {
        view.browser.rename("a.txt", "kept.txt").unwrap()
    });
    key(cx, rig.window, "ctrl-z");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    rig.view.update(cx, |view, _| {
        while view.browser.running() && std::time::Instant::now() < deadline {
            view.browser.poll();
            std::thread::yield_now();
        }
        assert!(!view.browser.running());
    });
    settle(cx, &rig.view);
    assert!(rig.notice(cx).is_none());
}

#[gpui_kit::test]
fn repeated_undo_retires_only_the_latest_offer(cx: &mut TestAppContext) {
    use std::sync::atomic::{AtomicU64, Ordering};
    let calls = Arc::new(AtomicU64::new(0));
    let retired = Arc::new(Mutex::new(Vec::new()));
    let (queries, log) = (calls.clone(), retired.clone());
    let hooks = crate::browser::RenameHooks {
        last: Arc::new(move || {
            let job = (queries.fetch_add(1, Ordering::Relaxed) + 1).min(2);
            Ok(Some(crate::browser::UndoOffer::retained(
                job,
                0,
                Vec::new(),
            )))
        }),
        retire: Arc::new(move |offer| {
            log.lock().unwrap().push(offer.job);
            Ok(())
        }),
        ..undo::Journal::default().hooks()
    };
    let rig = undo::open_with(
        cx,
        undo::files(),
        crate::browser::Operations::unlogged(),
        hooks,
    );
    calls.store(0, Ordering::Relaxed);
    key(cx, rig.window, "ctrl-z");
    key(cx, rig.window, "ctrl-z");
    settle(cx, &rig.view);
    assert_eq!(*retired.lock().unwrap(), [2]);
}
