use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};

use super::{Section, section_of};
use crate::prelude::*;
use crate::status_bar::StatusNotice;
use crate::window_view::{NavoWindow, UpdateHooks, UpdateView};

const POLL: Duration = Duration::from_secs(1);
const READY_POLL: Duration = Duration::from_secs(5);

/// Where a copy that cannot update itself gets the installer.
pub(super) const RELEASES_URL: &str = "https://github.com/trykA123/navo/releases/latest";

/// What Settings → Help shows about updates, read from the hooks.
pub(super) struct Updates {
    hooks: UpdateHooks,
    pub(super) view: UpdateView,
    is_polling: bool,
    /// The version of the ready notice on show.
    ready: Option<SharedString>,
}

impl Updates {
    pub(super) fn new(hooks: UpdateHooks) -> Self {
        Self {
            view: (hooks.state)(),
            hooks,
            is_polling: false,
            ready: None,
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

/// Waits, then asks for a ready version; false when the window is gone.
async fn take_ready(this: &WeakEntity<NavoWindow>, hooks: &UpdateHooks, cx: &mut AsyncApp) -> bool {
    cx.background_executor().timer(READY_POLL).await;
    let take = hooks.take_ready_notice.clone();
    let ready = cx.background_spawn(async move { take() }).await;
    this.update(cx, |view, cx| {
        if let Some(version) = ready {
            view.settings.updates.ready = Some(version);
            cx.notify();
        }
    })
    .is_ok()
}

impl NavoWindow {
    /// Shows the ready notice once per version, until the user's next command.
    pub(in crate::window_view) fn watch_ready_notice(&mut self, cx: &mut Context<Self>) {
        let hooks = self.settings.updates.hooks.clone();
        if (hooks.state)() == UpdateView::Hidden {
            return;
        }
        cx.spawn(async move |this, cx| while take_ready(&this, &hooks, cx).await {})
            .detach();
    }

    pub(in crate::window_view) fn update_notice(&self) -> Option<StatusNotice> {
        let version = self.settings.updates.ready.as_ref()?;
        Some(StatusNotice {
            text: navo_l10n::t!("settings-update-ready", version = version.as_ref()).into(),
            is_error: false,
        })
    }

    pub(in crate::window_view) fn forget_update_notice(&mut self) {
        self.settings.updates.ready = None;
    }

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
