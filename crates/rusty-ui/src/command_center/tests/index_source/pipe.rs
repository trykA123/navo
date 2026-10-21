use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use rusty_index::pipe::{PipeError, Reply};

use super::{hooks, request, source};
use crate::command_center::index_source::PipeQueries;
use crate::command_center::{Cancel, HitSink, IndexMode, ResultSource};

#[test]
fn a_hung_pipe_expires_and_auto_returns_host_rows() {
    let (entered, started) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let held = gate.clone();
    let mut hooks = hooks();
    hooks.pipe = PipeQueries::new(Arc::new(move |_, _, _| {
        entered.send(()).unwrap();
        let (lock, wake) = &*held;
        let _guard = wake
            .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(5), |open| !*open)
            .unwrap();
        Ok(Reply::default())
    }));
    let source = Arc::new(source(IndexMode::Auto, hooks));
    let (answer, received) = mpsc::channel();
    std::thread::spawn(move || {
        answer.send(super::run(&source)).unwrap();
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let hits = received.recv_timeout(Duration::from_secs(2));
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    assert_eq!(hits.unwrap().len(), 1);
}

#[test]
fn a_keystroke_cancels_the_old_pipe_query_and_discards_its_hits() {
    let (entered, started) = mpsc::channel();
    let (stopped, cancelled) = mpsc::channel();
    let mut hooks = hooks();
    hooks.pipe = PipeQueries::new(Arc::new(move |_, _, cancel| {
        entered.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cancel.is_cancelled() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        stopped.send(cancel.is_cancelled()).unwrap();
        Err(PipeError::Cancelled)
    }));
    let source = Arc::new(source(IndexMode::Auto, hooks));
    let old = Cancel::new();
    let old_query = old.clone();
    let sink = HitSink::new(1);
    let old_sink = sink.clone();
    let worker = std::thread::spawn(move || source.search(&request(1), &old_sink, &old_query));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    old.cancel();
    sink.begin(2);
    assert!(cancelled.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
    assert!(sink.drain(2).is_empty());
}

#[gpui_kit::test]
fn a_hung_pipe_does_not_delay_command_rows(cx: &mut gpui_kit::TestAppContext) {
    use crate::window_view::tests::{fake, open_options, options_for, press, two_panes};
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt;

    let (entered, started) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let held = gate.clone();
    let mut hooks = hooks();
    hooks.pipe = PipeQueries::new(Arc::new(move |_, _, _| {
        entered.send(()).unwrap();
        let _guard = held
            .1
            .wait_timeout_while(held.0.lock().unwrap(), Duration::from_secs(5), |open| {
                !*open
            })
            .unwrap();
        Err(PipeError::TimedOut)
    }));
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.search.index = hooks;
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "ctrl-k");
    cx.update_window(window, |_, window, cx| window.input("copy", cx))
        .unwrap();
    cx.run_until_parked();
    let has_commands = |cx: &mut gpui_kit::TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            (0usize..)
                .map_while(|at| window.try_find(("command-center-row", at)))
                .any(|row| row.label().is_some_and(|label| label.contains("Copy")))
        })
        .unwrap()
    };
    assert!(has_commands(cx));
    assert!(started.try_recv().is_err());
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let commands_remain = has_commands(cx);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    assert!(commands_remain);
}
