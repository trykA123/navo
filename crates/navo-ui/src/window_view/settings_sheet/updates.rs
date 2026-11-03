use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};

use super::{Section, section_of};
use crate::prelude::*;
use crate::window_view::{NavoWindow, UpdateHooks, UpdateView};

const POLL: Duration = Duration::from_secs(1);

/// What Settings → Help shows about updates, read from the hooks.
pub(super) struct Updates {
    hooks: UpdateHooks,
    pub(super) view: UpdateView,
    is_polling: bool,
}

impl Updates {
    pub(super) fn new(hooks: UpdateHooks) -> Self {
        Self {
            view: (hooks.state)(),
            hooks,
            is_polling: false,
        }
    }

    fn is_busy(&self) -> bool {
        matches!(
            self.view,
            UpdateView::Checking | UpdateView::Downloading { .. }
        )
    }
}

/// Waits a second, then re-reads the state; false when polling should stop.
async fn tick(this: &WeakEntity<NavoWindow>, cx: &mut AsyncApp) -> bool {
    cx.background_executor().timer(POLL).await;
    this.update(cx, |view, cx| view.poll_tick(cx))
        .unwrap_or(false)
}

impl NavoWindow {
    pub(in crate::window_view) fn refresh_updates(&mut self, cx: &mut Context<Self>) {
        self.settings.updates.view = (self.settings.updates.hooks.state)();
        self.poll_updates(cx);
        cx.notify();
    }

    pub(super) fn check_updates_now(&mut self, cx: &mut Context<Self>) {
        if self.settings.updates.is_busy() {
            return;
        }
        (self.settings.updates.hooks.check_now)();
        self.refresh_updates(cx);
    }

    fn poll_updates(&mut self, cx: &mut Context<Self>) {
        let updates = &mut self.settings.updates;
        if updates.is_polling || !updates.is_busy() {
            return;
        }
        updates.is_polling = true;
        cx.spawn(async move |this, cx| while tick(&this, cx).await {})
            .detach();
    }

    fn poll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let is_shown = self.settings.is_open && section_of(self.settings.row) == Section::Help;
        let updates = &mut self.settings.updates;
        if is_shown {
            updates.view = (updates.hooks.state)();
        }
        updates.is_polling = is_shown && updates.is_busy();
        cx.notify();
        updates.is_polling
    }
}
