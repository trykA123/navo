use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};
use rusty_core::{Command, Location, PaneId, Tab};

use super::FerriteWindow;
use crate::pane_chrome::{DrivePill, current_drive};
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
    pub(super) reconnecting: Vec<char>,
    errors: HashMap<PaneId, (Location, usize, SharedString)>,
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

    pub(super) fn error(&self, pane: PaneId, tab: &Tab) -> Option<SharedString> {
        self.errors
            .get(&pane)
            .filter(|(at, depth, _)| {
                at.same_place(tab.location()) && *depth == tab.history().back().len()
            })
            .map(|(_, _, message)| message.clone())
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
            self.report_pane_error(pane, "This drive is not connected.".into());
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
            Err(message) => self.report_pane_error(done.pane, message.into()),
        }
        cx.notify();
    }

    pub(super) fn report_pane_error(&mut self, pane: PaneId, message: SharedString) {
        let Some((location, depth)) = self.browser.state().pane(pane).map(|p| {
            let tab = p.active();
            (tab.location().clone(), tab.history().back().len())
        }) else {
            return;
        };
        self.drives.errors.insert(pane, (location, depth, message));
    }

    pub(super) fn drive_pill(&self, pane: PaneId, is_shown: bool, cx: &mut Context<Self>) -> Div {
        let drive = self
            .current_drive(pane)
            .and_then(|index| self.drives.list.get(index).cloned());
        let is_busy = drive
            .as_ref()
            .and_then(|drive| drive.letter)
            .is_some_and(|letter| self.drives.reconnecting.contains(&letter));
        let is_open = self
            .drive_menu
            .as_ref()
            .is_some_and(|menu| menu.pane == pane);
        let this = cx.weak_entity();
        let pill = DrivePill::new(("drive-pill", pane.get()), drive)
            .busy(is_busy)
            .open(is_open)
            .on_open(move |window, cx| {
                let _ = this.update(cx, |view, cx| {
                    view.focus_pane(pane);
                    view.open_drive_menu_for(pane, window, cx);
                });
            });
        let pill = Self::wire_drive_drops(pill, pane, cx);
        div()
            .relative()
            .flex_none()
            .when(is_shown, |host| host.child(pill))
            .children(self.drive_menu_view(pane, cx))
    }
}
