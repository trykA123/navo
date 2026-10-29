use std::sync::mpsc::{self, Receiver, Sender};

use gpui_kit::test::TestWindowExt;
use rusty_core::find::FileHits;
use rusty_core::{ProviderPath, TabId};
use rusty_fs::find::{FindProgress, FoundBatch, FoundFile};
use rusty_fs::{Attributes, CancelToken, EntryKind, FsEntry};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    started: Receiver<CancelToken>,
    finish: Sender<()>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    let (send, started) = mpsc::channel();
    let (finish, receive) = mpsc::channel();
    let receive = std::sync::Mutex::new(receive);
    view.update(cx, |view, _| {
        view.browser
            .set_find_walk(Arc::new(move |_, _, cancel, sink| {
                send.send(cancel.clone()).unwrap();
                let _ = sink(batch());
                receive.lock().unwrap().recv_timeout(WAIT).unwrap();
                Ok(progress())
            }));
    });
    Rig {
        window,
        view,
        started,
        finish,
    }
}

fn progress() -> FindProgress {
    FindProgress {
        hits: 4,
        matched: 2,
        ..FindProgress::default()
    }
}

fn batch() -> FoundBatch {
    FoundBatch {
        files: ["src/nested/a.rs", "root.rs"]
            .map(|name| FoundFile {
                relative: ProviderPath::from_text(name),
                entry: FsEntry {
                    name: ProviderPath::from_text(name),
                    kind: EntryKind::File,
                    size: 7,
                    modified_ms: None,
                    attributes: Attributes::default(),
                },
                hits: FileHits {
                    total: 2,
                    ..FileHits::default()
                },
            })
            .into(),
        progress: progress(),
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.finish.send(());
    }
}

impl Rig {
    fn pump(&self, cx: &mut TestAppContext) {
        self.view.update(cx, |view, cx| view.wake(cx));
        cx.run_until_parked();
        std::thread::yield_now();
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .unwrap();
        self.pump(cx);
    }

    fn start(&self, cx: &mut TestAppContext) -> TabId {
        self.press(cx, "alt-f7");
        cx.update_window(self.window, |_, window, cx| {
            window.input("needle ext:rs size:>1KB", cx)
        })
        .unwrap();
        self.press(cx, "ctrl-enter");
        self.started.recv_timeout(WAIT).unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            self.pump(cx);
            let tab = self
                .view
                .read_with(cx, |view, _| view.browser.state().active_tab().id());
            if self
                .view
                .read_with(cx, |view, _| view.browser.details(tab).len())
                == 2
            {
                return tab;
            }
            assert!(Instant::now() < deadline);
        }
    }

    fn label(&self, cx: &mut TestAppContext, id: impl Into<gpui_kit::ElementId>) -> String {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(id).label().unwrap().to_owned()
        })
        .unwrap()
    }

    fn end(&self, cx: &mut TestAppContext, tab: TabId) {
        self.finish.send(()).unwrap();
        let deadline = Instant::now() + WAIT;
        while self.view.read_with(cx, |view, _| {
            view.browser.find_results(tab).unwrap().running()
        }) {
            self.pump(cx);
            assert!(Instant::now() < deadline);
        }
    }
}

#[gpui_kit::test]
fn ctrl_enter_opens_query_chrome_with_filters(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let tab = rig.start(cx);
    rig.view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser
                .state()
                .active_tab()
                .location()
                .provider()
                .as_str(),
            "find"
        );
    });
    rig.view.read_with(cx, |view, _| {
        assert!(view.command_center_generation().is_none())
    });
    assert_eq!(rig.label(cx, 1usize), "needle");
    assert_eq!(
        rig.label(cx, "find-path"),
        "search: needle · ext:rs size:>1024"
    );
    rig.end(cx, tab);
}

#[gpui_kit::test]
fn status_tracks_running_and_completed_results(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let tab = rig.start(cx);
    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(
        rig.label(cx, ("status", pane.get())),
        "Searching · 4 hits in 2 files · Esc stop"
    );
    rig.end(cx, tab);
    assert_eq!(rig.label(cx, ("status", pane.get())), "4 hits in 2 files");
}

#[gpui_kit::test]
fn escape_stops_an_unfiltered_search_and_keeps_rows(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let tab = rig.start(cx);
    rig.press(cx, "escape");
    rig.view.read_with(cx, |view, _| {
        assert!(!view.browser.find_results(tab).unwrap().running());
        assert_eq!(view.browser.details(tab).len(), 2);
    });
    rig.finish.send(()).unwrap();
}

#[gpui_kit::test]
fn escape_stops_search_and_keeps_rows_even_with_a_filter(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let tab = rig.start(cx);
    rig.press(cx, "n");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view
            .browser
            .filter_text(tab)
            .map(str::to_owned)),
        Some("n".to_owned())
    );
    rig.press(cx, "escape");
    rig.view.read_with(cx, |view, _| {
        assert!(!view.browser.find_results(tab).unwrap().running());
        assert_eq!(view.browser.details(tab).len(), 1);
    });
    rig.press(cx, "escape");
    assert_eq!(
        rig.view
            .read_with(cx, |view, _| view.browser.details(tab).len()),
        2
    );
    rig.finish.send(()).unwrap();
}

#[gpui_kit::test]
fn folder_column_shows_relative_parent_and_name_shows_basename(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let tab = rig.start(cx);
    assert_eq!(rig.label(cx, "find-folder-header"), "Folder");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let row = window.within(("row", 0usize));
        assert_eq!(row.find("find-folder").label(), Some("src/nested"));
        assert_eq!(row.find("find-name").label(), Some("a.rs"));
    })
    .unwrap();
    rig.end(cx, tab);
}
