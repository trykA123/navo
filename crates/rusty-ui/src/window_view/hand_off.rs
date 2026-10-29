use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::{AsyncWindowContext, Global, WeakEntity};
use raw_window_handle::HasWindowHandle;
pub use raw_window_handle::RawWindowHandle;
use rusty_core::{Command, FsError, FsErrorKind, Location};
use rusty_fs::{FsEntry, Provider, ProviderOps};

use super::FerriteWindow;
use super::path_edit::{is_folder, last_name};
use crate::command_center::flights::{Answer, Ask, Flights, Rules};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

const CHECK_WAIT: Duration = Duration::from_secs(2);

pub(super) type Found = Result<FsEntry, FsError>;

pub(super) fn look_ups(wait: Duration) -> Flights<Found> {
    Flights::new(Rules {
        name: "look-up",
        wait,
        is_down,
        now: Arc::new(Instant::now),
    })
}

fn is_down(found: &Found) -> bool {
    found
        .as_ref()
        .is_err_and(|error| matches!(error.kind, FsErrorKind::NetworkLost | FsErrorKind::Timeout))
}

struct LookUps(Flights<Found>);

impl Default for LookUps {
    fn default() -> Self {
        Self(look_ups(CHECK_WAIT))
    }
}

impl Global for LookUps {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRequest {
    pub path: Option<Location>,
    pub left: Option<Location>,
    pub right: Option<Location>,
    pub view: Option<Location>,
    pub quit: bool,
}

#[derive(Clone, Debug)]
pub struct OpenSender(UnboundedSender<OpenRequest>);

impl OpenSender {
    pub fn send(&self, request: OpenRequest) -> bool {
        self.0.unbounded_send(request).is_ok()
    }
}

pub struct OpenInbox(UnboundedReceiver<OpenRequest>);

#[must_use]
pub fn open_requests() -> (OpenSender, OpenInbox) {
    let (sender, receiver) = unbounded();
    (OpenSender(sender), OpenInbox(receiver))
}

pub type WindowShown = Arc<dyn Fn(Option<RawWindowHandle>, bool) -> bool + Send + Sync>;

#[derive(Default)]
pub struct OpenHooks {
    pub path: Option<Location>,
    pub inbox: Option<OpenInbox>,
    pub on_shown: Option<Box<dyn FnOnce()>>,
    pub window_shown: Option<WindowShown>,
}

impl OpenHooks {
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

struct Arrival {
    request: OpenRequest,
    found: Option<Result<FsEntry, FsError>>,
    is_hand_off: bool,
}

pub(super) async fn look_up(
    provider: Arc<dyn ProviderOps>,
    location: Location,
    cx: &mut AsyncWindowContext,
) -> Option<Result<FsEntry, FsError>> {
    let flights = cx
        .update(|_, cx| cx.default_global::<LookUps>().0.clone())
        .ok()?;
    cx.background_spawn(async move { check(&flights, provider, &location) })
        .await
}

pub(super) fn check(
    flights: &Flights<Found>,
    provider: Arc<dyn Provider>,
    location: &Location,
) -> Option<Found> {
    let target = location.clone();
    let ask = Ask {
        key: location,
        since: flights.now(),
        cancel: &|| false,
    };
    match flights.get(&ask, move || provider.metadata(&target)) {
        Answer::Ready(found) => Some(found),
        Answer::Unreachable | Answer::Busy(_) | Answer::Full | Answer::Cancelled => None,
    }
}

async fn deliver(
    mut arrivals: impl futures::Stream<Item = (OpenRequest, bool)> + Unpin,
    provider: Arc<dyn ProviderOps>,
    this: WeakEntity<FerriteWindow>,
    cx: &mut AsyncWindowContext,
) {
    while let Some((request, is_hand_off)) = arrivals.next().await {
        let found = match request.path.clone() {
            Some(path) => look_up(provider.clone(), path, cx).await,
            None => None,
        };
        let arrival = Arrival {
            request,
            found,
            is_hand_off,
        };
        let taken = this.update_in(cx, |view, window, cx| {
            view.take_arrival(arrival, window, cx)
        });
        if taken.is_err() {
            return;
        }
    }
}

impl FerriteWindow {
    pub(super) fn listen_open(
        &mut self,
        hooks: OpenHooks,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.window_shown = hooks.window_shown;
        if let Some(on_shown) = hooks.on_shown {
            window.on_next_frame(move |window, _| window.on_next_frame(move |_, _| on_shown()));
        }
        let provider = self.provider.clone();
        let start = hooks.path.map(|path| OpenRequest {
            path: Some(path),
            ..OpenRequest::default()
        });
        let inbox = hooks
            .inbox
            .map_or_else(|| unbounded().1, |OpenInbox(inbox)| inbox);
        let arrivals = futures::stream::iter(start.map(|request| (request, false)))
            .chain(inbox.map(|request| (request, true)));
        cx.spawn_in(window, async move |this, cx| {
            deliver(arrivals, provider, this, cx).await;
        })
        .detach();
    }

    pub(super) fn set_window_shown(&self, window: &Window, shown: bool) -> bool {
        let handle = HasWindowHandle::window_handle(window)
            .ok()
            .map(|handle| handle.as_raw());
        self.window_shown
            .as_ref()
            .is_some_and(|hook| hook(handle, shown))
    }

    fn take_arrival(&mut self, arrival: Arrival, window: &mut Window, cx: &mut Context<Self>) {
        let Arrival {
            request,
            found,
            is_hand_off,
        } = arrival;
        if request.quit {
            self.quit_on_request(window, cx);
            return;
        }
        self.navigate_sides([request.left, request.right]);
        if let Some(path) = request.path {
            self.open_request_path(path, found);
        }
        if let Some(location) = request.view {
            self.view_location(location, window, cx);
        } else {
            self.focus_core_pane(window, cx);
        }
        if is_hand_off {
            self.set_window_shown(window, true);
            window.activate_window();
        }
        cx.notify();
    }

    pub(super) fn quit_on_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close(window, cx) {
            cx.quit();
        } else {
            self.set_window_shown(window, true);
            window.activate_window();
            cx.notify();
        }
    }

    fn navigate_sides(&mut self, folders: [Option<Location>; 2]) {
        if folders.iter().all(Option::is_none) {
            return;
        }
        let focused = self.browser.state().focused_pane();
        for (pane, folder) in self.panes().into_iter().zip(folders) {
            if let Some(location) = folder {
                self.focus_pane(pane);
                let _ = self.browser.command(&Command::PaneNavigate { location });
            }
        }
        self.focus_pane(focused);
    }

    fn open_request_path(&mut self, path: Location, found: Option<Result<FsEntry, FsError>>) {
        match found {
            Some(Err(error)) if error.kind == FsErrorKind::NotFound => {
                self.show_notice(StatusNotice {
                    text: format!("“{}” was not found", path.path().display()).into(),
                    is_error: true,
                });
            }
            Some(Ok(entry)) if !is_folder(&entry) => {
                if let (Some(name), Some(folder)) = (last_name(&path), path.parent()) {
                    self.browser.open_tab_at(folder, Some(name));
                }
            }
            _ => {
                self.browser.open_tab_at(path, None);
            }
        }
    }
}
