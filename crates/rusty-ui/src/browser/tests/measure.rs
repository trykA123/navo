use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

pub(super) struct Walk {
    cancel: CancelToken,
    release: Sender<bool>,
    observed: Receiver<bool>,
}

impl Walk {
    fn finish(self, wait_cancelled: bool) -> bool {
        self.release.send(wait_cancelled).unwrap();
        self.observed.recv_timeout(WAIT).unwrap()
    }

    fn crash(self) {
        drop(self.release);
    }
}

pub(super) fn gate_walk(cancel: &CancelToken, walks: &Sender<Walk>) {
    let (release, gate) = mpsc::channel();
    let (observed, result) = mpsc::channel();
    walks
        .send(Walk {
            cancel: cancel.clone(),
            release,
            observed: result,
        })
        .unwrap();
    match gate.recv_timeout(WAIT) {
        Ok(true) => {
            let _ = cancel.wait_cancelled(WAIT);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("size walk crashed on purpose"),
        Ok(false) | Err(mpsc::RecvTimeoutError::Timeout) => {}
    }
    let _ = observed.send(cancel.is_cancelled());
}

fn measuring() -> (Browser, Arc<FakeFs>, Receiver<Walk>, Receiver<()>) {
    let fs = fake();
    let (walks, started) = mpsc::channel();
    let (wake, woken) = mpsc::channel();
    let provider = Arc::new(Gated {
        fs: fs.clone(),
        open: AtomicBool::new(false),
        walks: Some(walks),
    });
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(move || {
            let _ = wake.send(());
        }),
    );
    browser.start();
    settle(&mut browser);
    while woken.try_recv().is_ok() {}
    browser.command(&Command::SelectionToggle).unwrap();
    (browser, fs, started, woken)
}

fn navigate(browser: &mut Browser, fs: &FakeFs, path: &str) {
    browser
        .command(&Command::PaneNavigate {
            location: fs.location(path),
        })
        .unwrap();
}

#[test]
fn leaving_a_folder_cancels_its_running_size_walk() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    navigate(&mut browser, &fs, "home/music");
    assert!(walk.finish(true));
    settle(&mut browser);
    assert!(!browser.loading());
}

#[test]
fn a_cancelled_size_does_not_show_after_returning_to_the_folder() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    navigate(&mut browser, &fs, "home/music");
    navigate(&mut browser, &fs, "home");
    assert!(walk.finish(true));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
    assert!(!browser.loading());
}

#[test]
fn a_size_delivered_before_leaving_does_not_show_on_a_later_visit() {
    let (mut browser, fs, started, woken) = measuring();
    assert!(!started.recv_timeout(WAIT).unwrap().finish(false));
    woken.recv_timeout(WAIT).unwrap();
    let measured = browser.receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(measured, Message::Measured(..)));
    navigate(&mut browser, &fs, "home/music");
    navigate(&mut browser, &fs, "home");
    settle(&mut browser);
    browser.sender.send(measured).unwrap();
    browser.poll();
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
}

#[test]
fn refreshing_the_same_folder_keeps_its_size_walk() {
    let (mut browser, _, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    browser.command(&Command::PaneRefresh).unwrap();
    let deadline = Instant::now() + WAIT;
    while browser.state().active_tab().pending_request().is_some() && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    assert!(browser.state().active_tab().pending_request().is_none());
    assert!(!walk.finish(false));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
    assert!(!browser.loading());
}

#[test]
fn closing_a_tab_cancels_its_size_walk() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    browser
        .command(&Command::PaneOpenTab {
            location: Some(fs.location("home/music")),
        })
        .unwrap();
    browser
        .command(&Command::PaneCloseTab { tab: Some(0) })
        .unwrap();
    assert!(walk.finish(true));
    settle(&mut browser);
    assert!(!browser.loading());
}

#[test]
fn tabs_measuring_at_once_do_not_cancel_each_other() {
    let (mut browser, fs, started, _) = measuring();
    let first = started.recv_timeout(WAIT).unwrap();
    browser.pools.queue(JobKind::Size).set_volume(
        volume_of(&fs.location("home/docs")),
        rusty_fs::VolumeKind::Ssd,
    );
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while browser.state().active_tab().listing().is_none() && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    browser.command(&Command::SelectionToggle).unwrap();
    let second = started.recv_timeout(WAIT).unwrap();
    assert!(!first.cancel.is_cancelled());
    assert!(!second.cancel.is_cancelled());
    browser
        .command(&Command::PaneCloseTab { tab: Some(0) })
        .unwrap();
    assert!(first.finish(true));
    assert!(!second.finish(false));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
    assert!(!browser.loading());
}

#[test]
fn a_crashed_size_walk_ends_the_spinner_and_shows_no_size() {
    let (mut browser, _, started, _) = measuring();
    started.recv_timeout(WAIT).unwrap().crash();
    settle(&mut browser);
    assert!(!browser.loading());
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
}
