use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};
use rusty_core::{Command, Location, PaneId};

use super::FerriteWindow;
use crate::pane_chrome::{DriveBar, current_drive};
use crate::prelude::*;

pub use crate::pane_chrome::{DriveIcon, DriveInfo};

const DRIVE_POLL: Duration = Duration::from_secs(2);

type Refresh = Arc<dyn Fn() -> Vec<DriveInfo> + Send + Sync>;
type Reconnect = Arc<dyn Fn(char, &str) -> Result<(), String> + Send + Sync>;

pub struct DriveHooks {
    pub refresh: Refresh,
    pub reconnect: Reconnect,
}

impl DriveHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            refresh: Arc::new(Vec::new),
            reconnect: Arc::new(|_, _| Err("Drives are not set up.".to_owned())),
        }
    }
}

pub(super) struct Drives {
    hooks: DriveHooks,
    pub(super) list: Rc<[DriveInfo]>,
    reconnecting: Vec<char>,
    errors: HashMap<PaneId, (Location, SharedString)>,
}

impl Drives {
    pub(super) fn new(hooks: DriveHooks) -> Self {
        Self {
            hooks,
            list: Rc::from([]),
            reconnecting: Vec::new(),
            errors: HashMap::new(),
        }
    }

    pub(super) fn error(&self, pane: PaneId, location: &Location) -> Option<SharedString> {
        self.errors
            .get(&pane)
            .filter(|(at, _)| at.same_place(location))
            .map(|(_, message)| message.clone())
    }
}

struct Reconnected {
    pane: PaneId,
    root: Location,
    letter: char,
    result: Result<(), String>,
}

impl FerriteWindow {
    pub(super) fn poll_drives(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while Self::refresh_drives(&this, cx).await {
                cx.background_executor().timer(DRIVE_POLL).await;
            }
        })
        .detach();
    }

    async fn refresh_drives(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> bool {
        let Ok(refresh) = this.read_with(cx, |view, _| view.drives.hooks.refresh.clone()) else {
            return false;
        };
        let list = cx.background_spawn(async move { refresh() }).await;
        this.update(cx, |view, cx| view.set_drives(list, cx))
            .is_ok()
    }

    fn set_drives(&mut self, list: Vec<DriveInfo>, cx: &mut Context<Self>) {
        if *self.drives.list != *list {
            self.drives.list = list.into();
            cx.notify();
        }
    }

    pub(super) fn current_drive(&self, pane: PaneId) -> Option<usize> {
        let pane = self.browser.state().pane(pane)?;
        current_drive(&self.drives.list, pane.active().location())
    }

    pub(super) fn free_on_drive(&self, pane: PaneId) -> Option<u64> {
        let index = self.current_drive(pane)?;
        self.drives.list.get(index)?.space.map(|(_, free)| free)
    }

    pub(super) fn pick_drive(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drive) = self.drives.list.get(index).cloned() else {
            return;
        };
        if drive.connected {
            self.apply(&Command::PaneGoDrive { root: drive.root }, window, cx);
            return;
        }
        self.reconnect(drive, window, cx);
    }

    fn reconnect(&mut self, drive: DriveInfo, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.browser.state().focused_pane();
        let (Some(letter), Some(remote)) = (drive.letter, drive.remote) else {
            self.drive_error(pane, "This drive is not connected.".into());
            cx.notify();
            return;
        };
        if self.drives.reconnecting.contains(&letter) {
            return;
        }
        self.drives.reconnecting.push(letter);
        let reconnect = self.drives.hooks.reconnect.clone();
        let root = drive.root;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { reconnect(letter, &remote) })
                .await;
            let done = Reconnected {
                pane,
                root,
                letter,
                result,
            };
            let _ = this.update_in(cx, |view, window, cx| view.reconnected(done, window, cx));
        })
        .detach();
        cx.notify();
    }

    fn reconnected(&mut self, done: Reconnected, window: &mut Window, cx: &mut Context<Self>) {
        self.drives
            .reconnecting
            .retain(|letter| *letter != done.letter);
        match done.result {
            Ok(()) => {
                self.focus_pane(done.pane);
                self.apply(&Command::PaneGoDrive { root: done.root }, window, cx);
            }
            Err(message) => self.drive_error(done.pane, message.into()),
        }
        cx.notify();
    }

    fn drive_error(&mut self, pane: PaneId, message: SharedString) {
        let Some(location) = self
            .browser
            .state()
            .pane(pane)
            .map(|p| p.active().location().clone())
        else {
            return;
        };
        self.drives.errors.insert(pane, (location, message));
    }

    pub(super) fn drive_bar(&self, pane: PaneId, cx: &mut Context<Self>) -> DriveBar {
        DriveBar::new(("drives", pane.get()), self.drives.list.clone())
            .current(self.current_drive(pane))
            .busy(self.drives.reconnecting.clone())
            .on_pick(
                cx.listener(move |this: &mut Self, index: &usize, window, cx| {
                    this.close_drive_menu();
                    this.path_edit = None;
                    this.focus_pane(pane);
                    this.pick_drive(*index, window, cx);
                }),
            )
    }
}
