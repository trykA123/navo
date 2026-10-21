use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::future::{Either, select};
use gpui_kit::{AsyncWindowContext, WeakEntity};
use rusty_core::{Command, FsError, FsErrorKind, Location};
use rusty_fs::{FsEntry, ProviderOps};

use super::FerriteWindow;
use super::path_edit::{is_folder, last_name};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

const CHECK_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRequest {
    pub path: Option<Location>,
    pub left: Option<Location>,
    pub right: Option<Location>,
    pub view: Option<Location>,
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

#[derive(Default)]
pub struct OpenHooks {
    pub path: Option<Location>,
    pub inbox: Option<OpenInbox>,
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
    let lookup = cx.background_spawn(async move { provider.metadata(&location) });
    let timeout = cx.background_executor().timer(CHECK_WAIT);
    match select(pin!(lookup), pin!(timeout)).await {
        Either::Left((found, _)) => Some(found),
        Either::Right(_) => None,
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

    fn take_arrival(&mut self, arrival: Arrival, window: &mut Window, cx: &mut Context<Self>) {
        let Arrival {
            request,
            found,
            is_hand_off,
        } = arrival;
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
            window.activate_window();
        }
        cx.notify();
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
