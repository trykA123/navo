use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_core::encoding::{TextEncoding, decode};

use super::{
    Log, Setup, editor_text, has_editor, hooks, press_only, redraw, text_of, text_window,
    window_on, window_with,
};
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::{FileHooks, FileText, Kinds};

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

#[gpui_kit::test]
fn f3_on_a_utf16_file_opens_the_text_editor_with_the_decoded_text(cx: &mut TestAppContext) {
    let bytes: Vec<u8> = "\u{FEFF}héllo wide world\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let files = hooks(Setup::new(bytes.clone()), &Log::default());
    let read = Arc::new(move |_: &rusty_core::Location| {
        let decoded = decode(&bytes, TextEncoding::default().encoding);
        Ok(FileText {
            read_only: !decoded.lossless,
            encoding: decoded.encoding,
            ..FileText::plain(decoded.text)
        })
    });
    let files = FileHooks { read, ..files };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "héllo wide world\n");
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

fn open_in_thread(
    name: &'static str,
    head: Vec<u8>,
    hooks: crate::window_view::viewer::FileHooks,
) -> std::thread::JoinHandle<bool> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use crate::window_view::viewer::open::load_while_alive;
    use crate::window_view::viewer::{Load, Sniff};

    std::thread::spawn(move || {
        let alive = Arc::new(());
        let sniff = Sniff {
            head: &head,
            name,
            len: head.len() as u64,
        };
        let kind = kind_for(name, &head);
        let location = rusty_core::Location::new(format!("/home/{name}")).expect("location");
        let cancel = AtomicBool::new(false);
        load_while_alive(&Arc::downgrade(&alive), &kind, || {
            kind.load(Load {
                hooks: &hooks,
                location: &location,
                sniff: &sniff,
                cancel: &cancel,
                encoding: None,
            })
        })
        .is_ok()
    })
}

#[test]
fn a_text_open_finishes_while_an_image_load_holds_the_lock() {
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use image::ImageFormat;

    use crate::window_view::viewer::image::fixtures::encoded;
    use crate::window_view::viewer::{FileHooks, FileText};

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
    let image = open_in_thread("b.png", encoded(ImageFormat::Png, 4, 4), hooks.clone());
    has_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("the image load started");
    let (done, finished) = channel();
    let text = open_in_thread("a.txt", b"hello\n".to_vec(), hooks);
    std::thread::spawn(move || {
        let _ = done.send(text.join().expect("text thread"));
    });
    let text_opened = finished.recv_timeout(Duration::from_secs(5));
    release.send(()).expect("release");
    assert!(image.join().expect("image thread"));
    assert_eq!(text_opened, Ok(true));
}

fn opening_flag(
    cx: &mut TestAppContext,
    view: &gpui_kit::Entity<crate::window_view::FerriteWindow>,
) -> Arc<std::sync::atomic::AtomicBool> {
    view.read_with(cx, |view, _| {
        let opening = view.viewer.as_ref()?.opening.as_ref()?;
        Some(opening.cancel_flag())
    })
    .expect("an open in progress")
}

#[gpui_kit::test]
fn closing_the_viewer_while_it_opens_sets_the_cancel_flag(cx: &mut TestAppContext) {
    use std::sync::atomic::Ordering;

    let (window, view, _) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    let flag = opening_flag(cx, &view);
    assert!(!flag.load(Ordering::Relaxed));
    press_only(cx, window, "escape");
    assert!(!viewer_open(cx, &view));
    assert!(flag.load(Ordering::Relaxed));
}

#[gpui_kit::test]
fn switching_the_view_while_it_loads_cancels_the_earlier_load(cx: &mut TestAppContext) {
    use std::sync::atomic::Ordering;

    let (window, view, _) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    settle(cx, &view);
    press_only(cx, window, "3");
    let first = opening_flag(cx, &view);
    assert!(!first.load(Ordering::Relaxed));
    press_only(cx, window, "1");
    assert!(viewer_open(cx, &view));
    assert!(first.load(Ordering::Relaxed));
}

const PDF_HEAD: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n";

#[derive(Clone, Default)]
struct Opens {
    count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    running: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    most: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Opens {
    fn hooks(&self, gate: std::sync::mpsc::Receiver<()>) -> crate::window_view::viewer::FileHooks {
        use std::sync::atomic::Ordering;
        use std::sync::{Arc, Mutex};

        let (opens, gate) = (self.clone(), Mutex::new(gate));
        crate::window_view::viewer::FileHooks {
            pdf: Arc::new(move |_, _| {
                let now = opens.running.fetch_add(1, Ordering::SeqCst) + 1;
                opens.most.fetch_max(now, Ordering::SeqCst);
                if opens.count.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = gate.lock().expect("gate").recv();
                }
                opens.running.fetch_sub(1, Ordering::SeqCst);
                Err(rusty_pdf::protocol::PdfError::TooLarge)
            }),
            ..crate::window_view::viewer::FileHooks::none()
        }
    }
}

fn open_pdf_in_thread(
    alive: &std::sync::Arc<()>,
    hooks: &crate::window_view::viewer::FileHooks,
) -> std::thread::JoinHandle<bool> {
    use std::sync::atomic::AtomicBool;

    use crate::window_view::viewer::open::load_while_alive;
    use crate::window_view::viewer::{Load, Sniff};

    let (weak, hooks) = (std::sync::Arc::downgrade(alive), hooks.clone());
    std::thread::spawn(move || {
        let kind = kind_for("a.pdf", PDF_HEAD);
        let sniff = Sniff {
            head: PDF_HEAD,
            name: "a.pdf",
            len: PDF_HEAD.len() as u64,
        };
        let location = rusty_core::Location::new("/home/a.pdf").expect("location");
        let cancel = AtomicBool::new(false);
        load_while_alive(&weak, &kind, || {
            kind.load(Load {
                hooks: &hooks,
                location: &location,
                sniff: &sniff,
                cancel: &cancel,
                encoding: None,
            })
        })
        .is_ok()
    })
}

#[test]
fn a_preview_sweep_over_pdfs_opens_one_helper_at_a_time_and_skips_passed_files() {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let opens = Opens::default();
    let (release, gate) = std::sync::mpsc::channel();
    let hooks = opens.hooks(gate);
    let first = Arc::new(());
    let holding = open_pdf_in_thread(&first, &hooks);
    while opens.count.load(Ordering::SeqCst) == 0 {
        std::thread::sleep(Duration::from_millis(2));
    }
    let passed: Vec<Arc<()>> = (0..3).map(|_| Arc::new(())).collect();
    let skipped: Vec<_> = passed
        .iter()
        .map(|alive| open_pdf_in_thread(alive, &hooks))
        .collect();
    let last = Arc::new(());
    let wanted = open_pdf_in_thread(&last, &hooks);
    std::thread::sleep(Duration::from_millis(50));
    drop(passed);
    release.send(()).expect("release");
    assert!(holding.join().expect("first"));
    for thread in skipped {
        assert!(!thread.join().expect("passed"));
    }
    assert!(wanted.join().expect("last"));
    assert_eq!(opens.count.load(Ordering::SeqCst), 2);
    assert_eq!(opens.most.load(Ordering::SeqCst), 1);
}

#[gpui_kit::test]
fn the_waiting_viewer_focus_ring_appears_only_with_root_focus(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;

    let (window, view, _) = super::text_window(cx, Setup::new(b"hello\n".to_vec()));
    assert!(!super::visible(cx, window, "viewer-focus-ring"));
    let focus = view.update(cx, |view, cx| {
        let viewer = view.viewer.as_mut().expect("viewer");
        viewer.body = None;
        let focus = viewer.focus.clone();
        cx.notify();
        focus
    });
    cx.update_window(window, |_, window, cx| window.focus(&focus, cx))
        .expect("window");
    redraw(cx, window);
    assert!(super::visible(cx, window, "viewer-focus-ring"));
    cx.update_window(window, |_, window, cx| window.blur(cx))
        .expect("window");
    redraw(cx, window);
    assert!(!super::visible(cx, window, "viewer-focus-ring"));
    assert!(super::visible(cx, window, "viewer-status"));
}
