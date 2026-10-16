use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::{AppState, Command, Direction, FsError, Location, PaneId, ProviderId, apply};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Provider, ProviderOps};

use super::super::{DriveHooks, DriveIcon, DriveInfo, FerriteWindow, StartOptions};
use super::open_options;
use crate::browser::Operations;
use crate::tokens::Mode;

const GIB: u64 = 1 << 30;

pub(super) struct DriveFs {
    fake: FakeFs,
    slow: Option<String>,
    released: Mutex<bool>,
    gate: Condvar,
}

impl DriveFs {
    fn new(slow: Option<&str>) -> Self {
        let fake = FakeFs::new();
        for folder in ["C/home/docs", "D/work", "Z/backup"] {
            fake.add_folder(folder);
        }
        fake.add_file("C/home/a.txt", 3);
        fake.add_file("C/home/docs/report.txt", 7);
        if let Some(slow) = slow {
            fake.add_folder(slow);
        }
        Self {
            fake,
            slow: slow.map(str::to_owned),
            released: Mutex::new(false),
            gate: Condvar::new(),
        }
    }

    pub(super) fn release(&self) {
        *self.released.lock().expect("gate") = true;
        self.gate.notify_all();
    }

    fn fake(&self, location: &Location) -> Location {
        let text = location
            .path()
            .to_text()
            .replacen(':', "", 1)
            .replace('\\', "/");
        self.fake.location(text.trim_end_matches('/'))
    }

    fn wait_if_slow(&self, folder: &Location, cancel: &CancelToken) {
        let path = self.fake(folder).path().to_text();
        if self.slow.as_deref() != Some(path.as_str()) {
            return;
        }
        let mut released = self.released.lock().expect("gate");
        while !*released && !cancel.is_cancelled() {
            released = self
                .gate
                .wait_timeout(released, Duration::from_millis(20))
                .expect("gate")
                .0;
        }
    }
}

impl Provider for DriveFs {
    fn id(&self) -> ProviderId {
        ProviderId::local()
    }

    fn capabilities(&self) -> Capabilities {
        self.fake.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.wait_if_slow(folder, cancel);
        self.fake.list(&self.fake(folder), sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fake.metadata(&self.fake(location))
    }
}

impl ProviderOps for DriveFs {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fake.create_folder(&self.fake(location))
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        let (source, target) = (self.fake(source), self.fake(target));
        self.fake.copy_file(&source, &target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fake.rename(&self.fake(source), &self.fake(target))
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fake.remove_file(&self.fake(location))
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fake.remove_link(&self.fake(location))
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fake.remove_empty_folder(&self.fake(location))
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fake.recycle(&self.fake(location))
    }
}

pub(super) fn local(text: &str) -> Location {
    Location::new(text).expect("valid location")
}

fn drive(letter: char, icon: DriveIcon, connected: bool) -> DriveInfo {
    DriveInfo {
        root: local(&format!("{letter}:\\")),
        letter: Some(letter),
        icon,
        label: None,
        remote: (icon == DriveIcon::Network).then(|| r"\\nas\share".to_owned()),
        connected,
        space: connected.then_some((100 * GIB, 40 * GIB)),
    }
}

pub(super) fn sample_drives() -> Vec<DriveInfo> {
    vec![
        drive('C', DriveIcon::Fixed, true),
        drive('D', DriveIcon::Removable, true),
        drive('Z', DriveIcon::Network, false),
    ]
}

pub(super) struct Rig {
    pub(super) window: AnyWindowHandle,
    pub(super) view: Entity<FerriteWindow>,
    pub(super) fs: Arc<DriveFs>,
    pub(super) calls: Arc<Mutex<Vec<(char, String)>>>,
}

pub(super) struct Setup {
    pub(super) reconnect: Result<(), String>,
    pub(super) slow: Option<&'static str>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            reconnect: Ok(()),
            slow: None,
        }
    }
}

fn two_panes_at_home() -> AppState {
    let mut state = AppState::new(local(r"C:\home"));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    state
}

pub(super) fn drive_window(cx: &mut TestAppContext, setup: Setup) -> Rig {
    let fs = Arc::new(DriveFs::new(setup.slow));
    let calls: Arc<Mutex<Vec<(char, String)>>> = Arc::default();
    let seen = calls.clone();
    let answer = setup.reconnect;
    let options = StartOptions {
        state: two_panes_at_home(),
        provider: fs.clone(),
        on_save: Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: DriveHooks {
            refresh: Arc::new(sample_drives),
            reconnect: Arc::new(move |letter, remote| {
                seen.lock()
                    .expect("calls")
                    .push((letter, remote.to_owned()));
                answer.clone()
            }),
        },
        view: None,
        listing_cache: rusty_cache::ListingCache::default(),
        cache_keys: rusty_cache::CacheKeys::default(),
        shell_menu: None,
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        fs,
        calls,
    }
}

impl Rig {
    pub(super) fn panes(&self, cx: &mut TestAppContext) -> Vec<PaneId> {
        self.view.read_with(cx, |view, _| view.panes())
    }

    pub(super) fn focused(&self, cx: &mut TestAppContext) -> PaneId {
        self.view
            .read_with(cx, |view, _| view.browser.state().focused_pane())
    }

    pub(super) fn location(&self, cx: &mut TestAppContext, pane: PaneId) -> Location {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            state
                .pane(pane)
                .map(|p| p.active().location().clone())
                .expect("pane")
        })
    }

    pub(super) fn click(
        &self,
        cx: &mut TestAppContext,
        scope: (&'static str, u64),
        item: (&'static str, usize),
    ) {
        self.frame(cx, |window, cx| window.within(scope).click(item, cx));
        super::settle(cx, &self.view);
    }

    pub(super) fn frame<R>(
        &self,
        cx: &mut TestAppContext,
        check: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            check(window, cx)
        })
        .expect("window")
    }

    pub(super) fn settle_until(
        &self,
        cx: &mut TestAppContext,
        done: impl Fn(&FerriteWindow) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.view.update(cx, |view, cx| view.wake(cx));
            cx.run_until_parked();
            if self.view.read_with(cx, |view, _| done(view)) || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
