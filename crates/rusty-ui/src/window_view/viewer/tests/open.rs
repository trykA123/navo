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

fn kind_for(name: &str, head: &[u8]) -> crate::window_view::viewer::Kind {
    let sniff = crate::window_view::viewer::Sniff {
        head,
        name,
        len: head.len() as u64,
    };
    crate::window_view::viewer::Kinds::builtin().choose(&sniff)[0].clone()
}

#[test]
fn image_loads_run_one_at_a_time_and_skip_viewers_that_closed() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Weak};
    use std::time::Duration;

    use image::ImageFormat;

    use crate::window_view::viewer::Loaded;
    use crate::window_view::viewer::image::fixtures::encoded;
    use crate::window_view::viewer::open::load_while_alive;

    let kind = kind_for("b.png", &encoded(ImageFormat::Png, 4, 4));
    let alive = Arc::new(());
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let weak = Arc::downgrade(&alive);
            let (running, most, kind) = (running.clone(), most.clone(), kind.clone());
            std::thread::spawn(move || {
                load_while_alive(&weak, &kind, || {
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
    let result = load_while_alive(&closed, &kind, || {
        called.store(true, Ordering::SeqCst);
        Ok(Box::new(()) as Loaded)
    });
    assert!(result.is_err());
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
fn a_text_open_finishes_while_an_image_load_holds_the_lock() {
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use image::ImageFormat;

    use crate::window_view::viewer::image::fixtures::encoded;
    use crate::window_view::viewer::open::load_while_alive;
    use crate::window_view::viewer::{FileHooks, FileText, Sniff};

    let png = encoded(ImageFormat::Png, 4, 4);
    let (entered, has_entered) = channel();
    let (release, released) = channel::<()>();
    let released = Mutex::new(released);
    let hooks = FileHooks {
        read: Arc::new(|_| Ok(FileText::plain("hello\n"))),
        bytes: Arc::new(move |_, _| {
            let _ = entered.send(());
            let _ = released.lock().expect("gate").recv();
            Ok(png.clone())
        }),
        ..FileHooks::none()
    };
    let open = move |name: &'static str, head: Vec<u8>, hooks: FileHooks| {
        std::thread::spawn(move || {
            let alive = Arc::new(());
            let sniff = Sniff {
                head: &head,
                name,
                len: head.len() as u64,
            };
            let kind = kind_for(name, &head);
            let location = rusty_core::Location::new(format!("/home/{name}")).expect("location");
            load_while_alive(&Arc::downgrade(&alive), &kind, || {
                kind.load(&hooks, &location, &sniff)
            })
            .is_ok()
        })
    };
    let image = open("b.png", encoded(ImageFormat::Png, 4, 4), hooks.clone());
    has_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("the image load started");
    let (done, finished) = channel();
    let text = open("a.txt", b"hello\n".to_vec(), hooks);
    std::thread::spawn(move || {
        let _ = done.send(text.join().expect("text thread"));
    });
    let text_opened = finished.recv_timeout(Duration::from_secs(5));
    release.send(()).expect("release");
    assert!(image.join().expect("image thread"));
    assert_eq!(text_opened, Ok(true));
}
