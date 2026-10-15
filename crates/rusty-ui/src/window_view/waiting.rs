use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, Role};
use rusty_core::{Command, PaneId, Tab, TabId};

use super::FerriteWindow;
use super::keys::{LeaveWaiting, WAITING};
use crate::prelude::*;
use crate::tokens;

const WAIT_AFTER: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct Waiting {
    watched: HashMap<TabId, u64>,
    late: HashMap<TabId, u64>,
}

impl Waiting {
    pub(super) fn is_waiting(&self, tab: &Tab) -> bool {
        tab.pending_request()
            .is_some_and(|request| self.late.get(&tab.id()) == Some(&request))
    }
}

impl FerriteWindow {
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
    }

    fn mark_late(&mut self, tab: TabId, request: u64, cx: &mut Context<Self>) {
        let still_pending = self
            .browser
            .state()
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request));
        if still_pending {
            self.waiting.late.insert(tab, request);
            cx.notify();
        }
    }

    pub(super) fn waiting_view(
        pane: PaneId,
        tab: &Tab,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = tokens::current(cx).clone();
        let path = tab.location().path().display().into_owned();
        div()
            .id(("waiting", pane.get()))
            .key_context(WAITING)
            .role(Role::Status)
            .aria_label(format!("Waiting for {path}"))
            .test_support()
            .track_focus(focus)
            .on_action(
                cx.listener(move |this: &mut Self, _: &LeaveWaiting, window, cx| {
                    this.focus_pane(pane);
                    this.apply(&Command::PaneBack, window, cx);
                }),
            )
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap(t.spacing.xs)
            .bg(t.colors.surface)
            .text_color(t.colors.muted)
            .child(format!("Waiting for {path}…"))
            .when(!tab.history().back().is_empty(), |this| {
                this.child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.faint)
                        .child("Esc goes back"),
                )
            })
            .into_any_element()
    }
}
