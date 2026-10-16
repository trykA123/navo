use gpui_kit::TestAppContext;

use super::{Setup, editor_text, has_editor, press_only, redraw, text_of, text_window, window_on};
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};

#[gpui_kit::test]
fn opening_shows_a_note_until_the_background_read_is_done(cx: &mut TestAppContext) {
    let (window, view, log) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    assert!(viewer_open(cx, &view));
    assert_eq!(text_of(cx, window, "viewer-status"), "Opening…");
    assert!(log.heads().is_empty());
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(log.heads(), ["a.txt"]);
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "hello\n");
}

#[gpui_kit::test]
fn escape_while_opening_closes_and_the_late_result_is_dropped(cx: &mut TestAppContext) {
    let (window, view, log) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    press_only(cx, window, "escape");
    assert!(!viewer_open(cx, &view));
    settle(cx, &view);
    redraw(cx, window);
    assert!(!viewer_open(cx, &view));
    assert_eq!(log.heads(), ["a.txt"]);
}

#[gpui_kit::test]
fn the_viewer_opens_read_only(cx: &mut TestAppContext) {
    let (window, view, log) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    type_in(cx, window, &view, "x");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(editor_text(cx, &view), "hello\n");
    assert!(log.written().is_empty());
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn after_ctrl_e_unsaved_edits_make_escape_warn(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    assert_eq!(editor_text(cx, &view), "xhello\n");
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
    assert!(text_of(cx, window, "viewer-message").starts_with("Unsaved changes."));
}

#[gpui_kit::test]
fn saving_runs_in_the_background_and_editing_goes_on(cx: &mut TestAppContext) {
    let (window, view, log) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    press_only(cx, window, "ctrl-s");
    assert_eq!(text_of(cx, window, "viewer-message"), "Saving…");
    assert!(log.written().is_empty());
    type_in(cx, window, &view, "y");
    redraw(cx, window);
    assert_eq!(log.written(), ["xhello\n"]);
    assert_eq!(text_of(cx, window, "viewer-message"), "Saved.");
    assert_eq!(editor_text(cx, &view), "xyhello\n");
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
}

#[test]
fn loads_run_one_at_a_time_and_skip_viewers_that_closed() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Weak};
    use std::time::Duration;

    use crate::window_view::viewer::Loaded;
    use crate::window_view::viewer::open::load_while_alive;

    let alive = Arc::new(());
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let weak = Arc::downgrade(&alive);
            let (running, most) = (running.clone(), most.clone());
            std::thread::spawn(move || {
                load_while_alive(&weak, || {
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    most.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok(Box::new(()) as Loaded)
                })
                .is_ok()
            })
        })
        .collect();
    for thread in threads {
        assert!(thread.join().expect("thread"));
    }
    assert_eq!(most.load(Ordering::SeqCst), 1);
    let closed: Weak<()> = Arc::downgrade(&Arc::new(()));
    let called = AtomicBool::new(false);
    let result = load_while_alive(&closed, || {
        called.store(true, Ordering::SeqCst);
        Ok(Box::new(()) as Loaded)
    });
    assert!(result.is_err());
    assert!(!called.load(Ordering::SeqCst));
}
