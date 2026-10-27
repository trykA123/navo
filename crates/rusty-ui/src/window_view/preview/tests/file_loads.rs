use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gpui_kit::{App, AppContext as _, TestAppContext, Window};
use rusty_core::Location;
use rusty_fs::{FakeFs, VolumeKind, volume_of};

use super::{Rig, WITHIN, open_on};
use crate::window_view::preview::Shown;
use crate::window_view::viewer::{Attach, Load, Loaded, Slot, Sniff, ViewerBody, ViewerKind};
use crate::window_view::{FileHooks, FileText, Head};

struct HeldKind {
    started: Sender<()>,
    release: Mutex<Receiver<()>>,
    finished: Sender<bool>,
    attached: Arc<AtomicUsize>,
}

impl ViewerKind for HeldKind {
    fn id(&self) -> &'static str {
        "held"
    }

    fn name(&self) -> &'static str {
        "Held"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.name.ends_with(".held") {
            100
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        self.started.send(()).unwrap();
        self.release.lock().unwrap().recv_timeout(WITHIN).unwrap();
        self.finished
            .send(load.cancel.load(Ordering::Acquire))
            .unwrap();
        Ok(Box::new(()))
    }

    fn attach(
        &self,
        _: Attach,
        _: &mut Window,
        _: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        self.attached.fetch_add(1, Ordering::SeqCst);
        Err("The superseded load was attached.".to_owned())
    }
}

struct Held {
    rig: Rig,
    started: Receiver<()>,
    release: Sender<()>,
    finished: Receiver<bool>,
    attached: Arc<AtomicUsize>,
}

fn held(cx: &mut TestAppContext) -> Held {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/first.held", 4);
    fs.add_file("home/second.txt", 4);
    let files = FileHooks {
        head: Arc::new(|_| {
            Ok(Head {
                bytes: b"next".to_vec(),
                len: 4,
            })
        }),
        read: Arc::new(|_| Ok(FileText::plain("next"))),
        ..FileHooks::none()
    };
    let rig = open_on(cx, &fs, fs.clone(), files);
    let (started_tx, started) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let (finished_tx, finished) = mpsc::channel();
    let attached = Arc::new(AtomicUsize::new(0));
    rig.view.update(cx, |view, _| {
        view.kinds = view.kinds.clone().with(HeldKind {
            started: started_tx,
            release: Mutex::new(release_rx),
            finished: finished_tx,
            attached: attached.clone(),
        });
    });
    Held {
        rig,
        started,
        release,
        finished,
        attached,
    }
}

fn body_is_ready(view: &crate::window_view::FerriteWindow) -> bool {
    matches!(view.preview.shown(), Some(Shown::File(file)) if file.body.is_some() && !file.is_loading())
}

#[gpui_kit::test]
fn moving_the_cursor_cancels_a_file_load_and_never_attaches_its_result(cx: &mut TestAppContext) {
    let held = held(cx);
    held.rig.open(cx);
    held.started.recv_timeout(WITHIN).unwrap();
    held.rig.key(cx, "down");
    assert!(held.rig.read(cx, |view| matches!(
        view.preview.shown(), Some(Shown::File(file)) if !file.is_loading()
    )));
    held.release.send(()).unwrap();
    assert!(held.finished.recv_timeout(WITHIN).unwrap());
    held.rig.rest(cx);
    assert!(held.rig.wait_until(cx, body_is_ready));
    assert_eq!(held.rig.shown_name(cx).as_deref(), Some("second.txt"));
    assert_eq!(held.attached.load(Ordering::SeqCst), 0);
}

fn start_at(cx: &mut TestAppContext, rig: &Rig, location: &Location) {
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            let mut target = view.cursor_target().unwrap();
            target.location = location.clone();
            let file = view.start_file(target, window, cx);
            view.preview.shown = Some(Shown::File(Box::new(file)));
        });
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_stalled_preview_read_on_one_volume_does_not_delay_another(cx: &mut TestAppContext) {
    let held = held(cx);
    let a = Location::new("A:\\first.held").unwrap();
    let b = Location::new("B:\\second.txt").unwrap();
    held.rig.view.update(cx, |view, _| {
        view.preview
            .reads
            .set_volume(volume_of(&a), VolumeKind::Hdd);
        view.preview
            .reads
            .set_volume(volume_of(&b), VolumeKind::Hdd);
    });
    start_at(cx, &held.rig, &a);
    held.started.recv_timeout(WITHIN).unwrap();
    let queued = Arc::new(AtomicBool::new(false));
    let ran = queued.clone();
    let owner = held.rig.read(cx, |view| {
        let owner = view.preview.reads.owner();
        view.preview.reads.submit(
            rusty_fs::JobSlot {
                volume: volume_of(&a),
                priority: rusty_fs::Priority::Visible,
                owner: owner.id(),
            },
            Box::new(move |_| {
                ran.store(true, Ordering::SeqCst);
            }),
        );
        owner
    });
    held.rig.key(cx, "down");
    start_at(cx, &held.rig, &b);
    assert!(held.rig.wait_until(cx, body_is_ready));
    assert!(
        !queued.load(Ordering::SeqCst),
        "volume A must still be occupied by the file read"
    );
    held.release.send(()).unwrap();
    assert!(held.finished.recv_timeout(WITHIN).unwrap());
    assert_eq!(held.rig.shown_name(cx).as_deref(), Some("second.txt"));
    assert_eq!(held.attached.load(Ordering::SeqCst), 0);
    drop(owner);
}

#[gpui_kit::test]
fn a_file_load_error_clears_loading_and_shows_the_error_state(cx: &mut TestAppContext) {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/broken.txt", 4);
    let rig = open_on(
        cx,
        &fs,
        fs.clone(),
        FileHooks {
            head: Arc::new(|_| Err("The test file is unreadable.".to_owned())),
            ..FileHooks::none()
        },
    );
    rig.open(cx);
    assert!(rig.wait_until(cx, |view| matches!(
        view.preview.shown(),
        Some(Shown::File(file)) if file.message.is_some() && !file.is_loading() && file.body.is_none()
    )));
    assert!(rig.shows(cx, "preview-message"));
}

#[gpui_kit::test]
fn a_crashed_file_load_shows_an_error_instead_of_loading(cx: &mut TestAppContext) {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/crash.txt", 4);
    let rig = open_on(
        cx,
        &fs,
        fs.clone(),
        FileHooks {
            head: Arc::new(|_| panic!("the load crashed on purpose")),
            ..FileHooks::none()
        },
    );
    rig.open(cx);
    assert!(rig.wait_until(cx, |view| matches!(
        view.preview.shown(),
        Some(Shown::File(file)) if file.message.is_some() && !file.is_loading()
    )));
}
