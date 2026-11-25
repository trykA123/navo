use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, Role};
use navo_core::{Command, Location, PaneId, Tab, TabId};

use super::NavoWindow;
use super::keys::{LeaveWaiting, RetryListing, WAITING};
use crate::pane_chrome::{StateCard, state_card};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName};
use crate::status_bar::ListingMark;
use crate::tokens;

const WAIT_AFTER: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct Waiting {
    watched: HashMap<TabId, u64>,
    late: HashMap<TabId, u64>,
}

impl Waiting {
    pub(super) fn is_waiting(&self, tab: &Tab) -> bool {
        tab.listing().is_none()
            && tab
                .pending_request()
                .is_some_and(|request| self.late.get(&tab.id()) == Some(&request))
    }
}

pub(super) struct Waited {
    pane: PaneId,
    tab: Tab,
    is_offline: bool,
}

fn millis(at: SystemTime) -> Option<i64> {
    let since = at.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// The text a waiting pane shows for `location`: a readable path, also inside an archive.
fn shown_path(location: &Location) -> String {
    navo_fs::archive::shown_path(location).unwrap_or_else(|| location.path().display().into_owned())
}

impl Waited {
    fn label_and_title(&self) -> (String, String) {
        let path = shown_path(self.tab.location());
        if self.is_offline {
            (
                format!("{path} is not answering · retrying"),
                "Not answering · retrying".to_owned(),
            )
        } else {
            (
                format!("Waiting for {path}"),
                format!("Waiting for {path}…"),
            )
        }
    }

    fn hint(&self) -> Option<&'static str> {
        let can_go_back = !self.tab.history().back().is_empty();
        match (self.is_offline, can_go_back) {
            (true, true) => Some("Enter retries · Esc goes back"),
            (true, false) => Some("Enter retries"),
            (false, true) => Some("Esc goes back"),
            (false, false) => None,
        }
    }
}

impl NavoWindow {
    pub(super) fn watch_listings(&mut self, cx: &mut Context<Self>) {
        let state = self.browser.state();
        let pending: Vec<(TabId, u64)> = self
            .panes()
            .into_iter()
            .filter_map(|pane| state.pane(pane).map(|p| p.active()))
            .filter_map(|tab| tab.pending_request().map(|request| (tab.id(), request)))
            .filter(|(tab, request)| self.waiting.watched.get(tab) != Some(request))
            .collect();
        for (tab, request) in pending {
            self.waiting.watched.insert(tab, request);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(WAIT_AFTER).await;
                let _ = this.update(cx, |view, cx| view.mark_late(tab, request, cx));
            })
            .detach();
        }
        for (tab, request, first) in self.browser.take_new_offline() {
            Self::keep_retrying(tab, request, first, cx);
        }
    }

    fn mark_late(&mut self, tab: TabId, request: u64, cx: &mut Context<Self>) {
        let still_pending = self
            .browser
            .state()
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request));
        if !still_pending {
            return;
        }
        self.waiting.late.insert(tab, request);
        if let Some(first) = self.browser.listing_overdue(tab, request) {
            Self::keep_retrying(tab, request, first, cx);
        }
        cx.notify();
    }

    fn keep_retrying(tab: TabId, request: u64, first: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut delay = first;
            loop {
                cx.background_executor().timer(delay).await;
                match this.update(cx, |view, cx| view.retry_tick(tab, request, cx)) {
                    Ok(Some(next)) => delay = next,
                    _ => return,
                }
            }
        })
        .detach();
    }

    pub(super) fn waited(&self, pane: PaneId, tab: &Tab) -> Option<Waited> {
        let is_offline = tab.listing().is_none() && self.browser.offline(tab.id()).is_some();
        (is_offline || self.waiting.is_waiting(tab)).then(|| Waited {
            pane,
            tab: tab.clone(),
            is_offline,
        })
    }

    pub(super) fn listing_mark(&self, tab: &Tab) -> ListingMark {
        match self.browser.offline(tab.id()) {
            Some(offline) if tab.listing().is_some() => ListingMark::Offline {
                cached_at_ms: offline.cached_at.and_then(millis),
            },
            _ if self.browser.is_cached(tab.id()) => ListingMark::Cached,
            _ => ListingMark::Fresh,
        }
    }

    fn retry_tick(&mut self, tab: TabId, request: u64, cx: &mut Context<Self>) -> Option<Duration> {
        let next = self.browser.retry_offline(tab, request);
        cx.notify();
        next
    }

    fn retry_pane(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let tab = self.browser.state().pane(pane).map(|p| p.active().id());
        if tab.is_some_and(|tab| self.browser.retry_listing(tab)) {
            cx.notify();
        }
    }

    pub(super) fn waiting_view(
        waited: &Waited,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = tokens::current(cx).clone();
        let pane = waited.pane;
        let path = shown_path(waited.tab.location());
        let (label, title) = waited.label_and_title();
        let subtitle: SharedString = match (waited.is_offline, waited.hint()) {
            (true, Some(hint)) => format!("{path} · {hint}").into(),
            (true, None) => path.into(),
            (false, Some(hint)) => hint.into(),
            (false, None) => path.into(),
        };
        let glyph = if waited.is_offline {
            IconName::TriangleAlert
        } else {
            IconName::RefreshCw
        };
        let action = waited.is_offline.then(|| {
            Self::retry_button(pane, cx)
                .variant(ButtonVariant::Primary)
                .into_any_element()
        });
        div()
            .id(("waiting", pane.get()))
            .key_context(WAITING)
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .track_focus(focus)
            .on_action(
                cx.listener(move |this: &mut Self, _: &LeaveWaiting, window, cx| {
                    this.focus_pane(pane);
                    this.apply(&Command::PaneBack, window, cx);
                }),
            )
            .on_action(
                cx.listener(move |this: &mut Self, _: &RetryListing, _, cx| {
                    this.retry_pane(pane, cx);
                }),
            )
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(state_card(
                &t,
                StateCard {
                    card_id: ("waiting-card", pane.get()).into(),
                    icon: glyph,
                    is_error: waited.is_offline,
                    title: title.into(),
                    subtitle,
                    action,
                },
            ))
            .into_any_element()
    }

    fn retry_button(pane: PaneId, cx: &mut Context<Self>) -> Button {
        let view = cx.weak_entity();
        Button::new(("retry", pane.get()), navo_l10n::tr("pane-error-retry")).on_click(
            move |_, _, cx| {
                let _ = view.update(cx, |view, cx| view.retry_pane(pane, cx));
            },
        )
    }
}
