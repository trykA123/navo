use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::{FutureExt as _, StreamExt as _};
use gpui_kit::{AsyncApp, WeakEntity};
use rusty_core::{Command, Location, PaneId, Tab};

use super::FerriteWindow;
use crate::pane_chrome::{DrivePill, current_drive};
use crate::prelude::*;

pub use crate::pane_chrome::{DriveIcon, DriveInfo};

const DRIVE_SETTLE: Duration = Duration::from_millis(300);
const DRIVE_MAX_WAIT: Duration = Duration::from_secs(2);

type Refresh = Arc<dyn Fn() -> Vec<DriveInfo> + Send + Sync>;
type Reconnect = Arc<dyn Fn(char, &str) -> Result<(), String> + Send + Sync>;

pub struct DriveHooks {
    pub refresh: Refresh,
    pub reconnect: Reconnect,
    pub changes: Option<DriveChanges>,
}

impl DriveHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            refresh: Arc::new(Vec::new),
            reconnect: Arc::new(|_, _| Err("Drives are not set up.".to_owned())),
            changes: None,
        }
    }
}

#[derive(Clone)]
pub struct DriveSignal(UnboundedSender<()>);

impl DriveSignal {
    pub fn send(&self) {
        let _ = self.0.unbounded_send(());
    }
}

pub struct DriveChanges {
    signals: Option<UnboundedReceiver<()>>,
    _watch: Option<Box<dyn Any>>,
}

impl DriveChanges {
    #[must_use]
    pub fn channel() -> (DriveSignal, Self) {
        let (sender, signals) = unbounded();
        let changes = Self {
            signals: Some(signals),
            _watch: None,
        };
        (DriveSignal(sender), changes)
    }

    #[must_use]
    pub fn keeping(self, watch: Box<dyn Any>) -> Self {
        Self {
            _watch: Some(watch),
            ..self
        }
    }
}

pub(super) struct Drives {
    hooks: DriveHooks,
    pub(super) list: Rc<[DriveInfo]>,
    pub(super) reconnecting: Vec<char>,
    errors: HashMap<PaneId, (Location, usize, SharedString)>,
    refresh: RefreshState,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum RefreshState {
    #[default]
    Idle,
    Running,
    Again,
}

impl Drives {
    pub(super) fn new(hooks: DriveHooks) -> Self {
        Self {
            hooks,
            list: Rc::from([]),
            reconnecting: Vec::new(),
            errors: HashMap::new(),
            refresh: RefreshState::Idle,
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

async fn settle_drive_changes(signals: &mut UnboundedReceiver<()>, cx: &AsyncApp) {
    let started = cx.background_executor().now();
    let maximum = started + DRIVE_MAX_WAIT;
    let mut deadline = started + DRIVE_SETTLE;
    loop {
        let now = cx.background_executor().now();
        if now >= deadline {
            break;
        }
        let timer = cx.background_executor().timer(deadline - now).fuse();
        futures::pin_mut!(timer);
        futures::select_biased! {
            () = timer => break,
            signal = signals.next().fuse() => {
                if signal.is_none() {
                    break;
                }
                deadline = (cx.background_executor().now() + DRIVE_SETTLE).min(maximum);
            }
        }
    }
}

impl FerriteWindow {
    pub(super) fn watch_drives(&mut self, cx: &mut Context<Self>) {
        self.refresh_drives(cx);
        let changes = self.drives.hooks.changes.as_mut();
        let Some(mut signals) = changes.and_then(|changes| changes.signals.take()) else {
            return;
        };
        cx.spawn(async move |this, cx| Self::follow_volumes(&this, &mut signals, cx).await)
            .detach();
    }

    async fn follow_volumes(
        this: &WeakEntity<Self>,
        signals: &mut UnboundedReceiver<()>,
        cx: &mut AsyncApp,
    ) {
        while signals.next().await.is_some() {
            settle_drive_changes(signals, cx).await;
            if this.update(cx, |view, cx| view.refresh_drives(cx)).is_err() {
                return;
            }
        }
    }

    pub(super) fn refresh_drives(&mut self, cx: &mut Context<Self>) {
        if self.drives.refresh != RefreshState::Idle {
            self.drives.refresh = RefreshState::Again;
            return;
        }
        self.drives.refresh = RefreshState::Running;
        let refresh = self.drives.hooks.refresh.clone();
        cx.spawn(async move |this, cx| {
            let list = cx.background_spawn(async move { refresh() }).await;
            let _ = this.update(cx, |view, cx| view.drives_read(list, cx));
        })
        .detach();
    }

    fn drives_read(&mut self, list: Vec<DriveInfo>, cx: &mut Context<Self>) {
        let again = self.drives.refresh == RefreshState::Again;
        self.drives.refresh = RefreshState::Idle;
        self.set_drives(list, cx);
        if again {
            self.refresh_drives(cx);
        }
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
        self.refresh_drives(cx);
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
