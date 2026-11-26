use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};

use super::{Section, section_of};
use crate::prelude::*;
use crate::settings::RollbackState;
use crate::status_bar::StatusNotice;
use crate::window_view::{NavoWindow, RollbackTargetView, UpdateHooks, UpdateView};

const POLL: Duration = Duration::from_secs(1);
const READY_FAST_POLL: Duration = Duration::from_secs(5);
const READY_SLOW_POLL: Duration = Duration::from_secs(60);
const READY_FAST_TRIES: u32 = 12;

struct ReadyPoll {
    delay: Duration,
    misses: u32,
}

impl Default for ReadyPoll {
    fn default() -> Self {
        Self {
            delay: READY_FAST_POLL,
            misses: 0,
        }
    }
}

impl ReadyPoll {
    fn delay(&self) -> Duration {
        self.delay
    }

    fn miss(&mut self) {
        self.misses += 1;
        if self.misses >= READY_FAST_TRIES {
            self.delay = READY_SLOW_POLL;
        }
    }

    fn hit(&mut self) {
        self.misses = 0;
        self.delay = READY_FAST_POLL;
    }
}

/// Where a copy that cannot update itself gets the installer.
pub(super) const RELEASES_URL: &str = "https://github.com/trykA123/navo/releases/latest";

/// What state/version the title-bar update pill was dismissed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::window_view) enum PillKey {
    Downloading(SharedString),
    Ready(SharedString),
    Failed(super::super::UpdateFailure),
}

impl PillKey {
    fn of(view: &UpdateView) -> Option<Self> {
        match view {
            UpdateView::Downloading { version, .. } => Some(Self::Downloading(version.clone())),
            UpdateView::Ready { version } => Some(Self::Ready(version.clone())),
            UpdateView::Failed(failure) => Some(Self::Failed(failure.clone())),
            UpdateView::Hidden
            | UpdateView::Off
            | UpdateView::Idle
            | UpdateView::Checking
            | UpdateView::UpToDate => None,
        }
    }
}

/// What Settings → Help shows about updates, read from the hooks.
pub(in crate::window_view) struct Updates {
    pub(in crate::window_view) hooks: UpdateHooks,
    pub(in crate::window_view) view: UpdateView,
    skipped_version: Option<SharedString>,
    rollback_targets: Vec<RollbackTargetView>,
    rollback_open: bool,
    confirming_rollback: Option<SharedString>,
    rollback_error: Option<SharedString>,
    is_polling: bool,
    /// The version of the ready notice on show.
    ready: Option<SharedString>,
    dismissed_pill: Option<PillKey>,
    pub(in crate::window_view) restart: super::super::closing::Restart,
    pub(in crate::window_view) restart_dialogs: u64,
    pub(in crate::window_view) restart_watch: Option<gpui_kit::Subscription>,
    pub(in crate::window_view) restart_focus: Option<gpui_kit::FocusHandle>,
    pub(in crate::window_view) restart_task: Option<gpui_kit::Task<()>>,
}

impl Updates {
    pub(super) fn new(hooks: UpdateHooks) -> Self {
        Self {
            view: (hooks.state)(),
            skipped_version: (hooks.skipped_version)(),
            rollback_targets: (hooks.rollback_targets)(),
            rollback_open: false,
            confirming_rollback: None,
            rollback_error: None,
            hooks,
            is_polling: false,
            ready: None,
            dismissed_pill: None,
            restart: super::super::closing::Restart::Idle,
            restart_dialogs: 0,
            restart_task: None,
            restart_watch: None,
            restart_focus: None,
        }
    }

    pub(super) fn rollback_state(&self) -> RollbackState {
        RollbackState {
            skipped_version: self.skipped_version.clone(),
            targets: self.rollback_targets.clone(),
            is_open: self.rollback_open,
            confirming: self.confirming_rollback.clone(),
            error: self.rollback_error.clone(),
        }
    }

    pub(super) fn reset_rollback_ui(&mut self) {
        self.rollback_open = false;
        self.confirming_rollback = None;
        self.rollback_error = None;
    }

    pub(in crate::window_view) fn set_view(&mut self, view: UpdateView) {
        if PillKey::of(&view) != self.dismissed_pill {
            self.dismissed_pill = None;
        }
        self.view = view;
    }

    pub(in crate::window_view) fn is_pill_dismissed(&self) -> bool {
        PillKey::of(&self.view).is_some_and(|key| self.dismissed_pill.as_ref() == Some(&key))
    }

    pub(in crate::window_view) fn dismiss_pill(&mut self) {
        self.dismissed_pill = PillKey::of(&self.view);
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
async fn take_ready(
    this: &WeakEntity<NavoWindow>,
    hooks: &UpdateHooks,
    poll: &mut ReadyPoll,
    cx: &mut AsyncApp,
) -> bool {
    cx.background_executor().timer(poll.delay()).await;
    let (take, state) = (hooks.take_ready_notice.clone(), hooks.state.clone());
    let ready = cx
        .background_spawn(async move { take().map(|version| (version, state())) })
        .await;
    let found = ready.is_some();
    let alive = this
        .update(cx, |view, cx| {
            if let Some((version, state)) = ready {
                view.settings.updates.ready = Some(version);
                view.settings.updates.set_view(state);
                cx.notify();
            }
        })
        .is_ok();
    if found {
        poll.hit();
    } else {
        poll.miss();
    }
    alive
}

impl NavoWindow {
    /// Shows the ready notice once per version, until the user's next command.
    pub(in crate::window_view) fn watch_ready_notice(&mut self, cx: &mut Context<Self>) {
        let hooks = self.settings.updates.hooks.clone();
        if self.identity.install == crate::app_menu::Install::Portable
            || (hooks.state)() == UpdateView::Hidden
        {
            return;
        }
        let mut poll = ReadyPoll::default();
        cx.spawn(
            async move |this, cx| {
                while take_ready(&this, &hooks, &mut poll, cx).await {}
            },
        )
        .detach();
    }

    pub(in crate::window_view) fn restart_wait_notice(&self) -> Option<StatusNotice> {
        (self.settings.updates.restart == super::super::closing::Restart::Waiting).then(|| {
            StatusNotice {
                text: navo_l10n::t!("update-restart-waiting", count = self.restart_job_count())
                    .into(),
                is_error: false,
            }
        })
    }

    pub(in crate::window_view) fn update_notice(&self) -> Option<StatusNotice> {
        if let Some(waiting) = self.restart_wait_notice() {
            return Some(waiting);
        }
        let version = self.settings.updates.ready.as_ref()?;
        if self.restart_available() {
            return Some(StatusNotice {
                text: navo_l10n::t!("settings-update-ready-restart", version = version.as_ref())
                    .into(),
                is_error: false,
            });
        }
        Some(StatusNotice {
            text: navo_l10n::t!(
                "settings-update-ready",
                version = version.as_ref(),
                tray = if self.values.keeps_in_tray() {
                    "yes"
                } else {
                    "no"
                }
            )
            .into(),
            is_error: false,
        })
    }

    pub(in crate::window_view) fn forget_update_notice(&mut self) {
        self.settings.updates.ready = None;
    }

    pub(in crate::window_view) fn dismiss_update_pill(&mut self, cx: &mut Context<Self>) {
        self.settings.updates.dismiss_pill();
        cx.notify();
    }

    pub(in crate::window_view) fn refresh_updates(&mut self, cx: &mut Context<Self>) {
        let is_portable = self.identity.install == crate::app_menu::Install::Portable;
        let next = if is_portable {
            UpdateView::Hidden
        } else {
            (self.settings.updates.hooks.state)()
        };
        self.settings.updates.set_view(next);
        if is_portable {
            self.settings.updates.skipped_version = None;
            self.settings.updates.rollback_targets.clear();
        } else {
            self.settings.updates.skipped_version = (self.settings.updates.hooks.skipped_version)();
            self.settings.updates.rollback_targets =
                (self.settings.updates.hooks.rollback_targets)();
        }
        self.poll_updates(cx);
        cx.notify();
    }

    pub(super) fn clear_skipped_version(&mut self, cx: &mut Context<Self>) {
        (self.settings.updates.hooks.clear_skipped_version)();
        self.settings.updates.skipped_version = (self.settings.updates.hooks.skipped_version)();
        cx.notify();
    }

    pub(super) fn toggle_rollback_fold(&mut self) {
        self.settings.updates.rollback_open = !self.settings.updates.rollback_open;
        if !self.settings.updates.rollback_open {
            self.settings.updates.confirming_rollback = None;
        }
        self.settings.updates.rollback_error = None;
    }

    pub(super) fn prompt_rollback(&mut self, version: SharedString) {
        self.settings.updates.confirming_rollback = Some(version);
        self.settings.updates.rollback_error = None;
    }

    pub(super) fn cancel_rollback(&mut self) {
        self.settings.updates.confirming_rollback = None;
    }

    pub(super) fn confirm_rollback(&mut self, version: &str, cx: &mut Context<Self>) {
        self.settings.updates.confirming_rollback = None;
        match (self.settings.updates.hooks.apply_rollback)(version) {
            Ok(()) => {
                self.settings.updates.rollback_error = None;
                self.refresh_updates(cx);
            }
            Err(reason) => {
                self.settings.updates.rollback_error = Some(reason);
            }
        }
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
        let is_downloading = matches!(updates.view, UpdateView::Downloading { .. });
        if is_shown || is_downloading {
            updates.set_view((updates.hooks.state)());
        }
        let still_downloading = matches!(updates.view, UpdateView::Downloading { .. });
        updates.is_polling = (is_shown || still_downloading) && updates.is_busy();
        cx.notify();
        updates.is_polling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_poll_backs_off_after_a_minute_and_resets_on_a_hit() {
        let mut poll = ReadyPoll::default();
        for _ in 0..READY_FAST_TRIES {
            assert_eq!(poll.delay(), READY_FAST_POLL);
            poll.miss();
        }
        assert_eq!(poll.delay(), READY_SLOW_POLL);
        poll.miss();
        assert_eq!(poll.delay(), READY_SLOW_POLL);
        poll.hit();
        assert_eq!(poll.delay(), READY_FAST_POLL);
        assert_eq!(poll.misses, 0);
    }
}
