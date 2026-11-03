use std::sync::Arc;

use crate::prelude::*;

pub type UpdateState = Arc<dyn Fn() -> UpdateView + Send + Sync>;
pub type UpdateAction = Arc<dyn Fn() + Send + Sync>;
pub type RestartUpdate = Arc<dyn Fn() -> Result<(), UpdateFailure> + Send + Sync>;
pub type TakeReadyNotice = Arc<dyn Fn() -> Option<SharedString> + Send + Sync>;

/// Why the last update check or download failed (D22h).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateFailure {
    Offline,
    RateLimited {
        next_try: SharedString,
    },
    BadFeed,
    Interrupted,
    Damaged,
    Busy,
    /// The restart could not hand the staged package to the updater.
    Apply,
    /// Another process of this install is open, so the restart waits.
    OthersRunning,
}

/// What Settings → Help shows about updates (D22g).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateView {
    Hidden,
    Off,
    Idle,
    Checking,
    UpToDate,
    Downloading { version: SharedString, percent: u8 },
    Ready { version: SharedString },
    Failed(UpdateFailure),
}

/// `take_ready_notice` returns a ready version once per version, app-wide.
#[derive(Clone)]
pub struct UpdateHooks {
    pub state: UpdateState,
    pub check_now: UpdateAction,
    pub restart: RestartUpdate,
    pub take_ready_notice: TakeReadyNotice,
}

impl UpdateHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            state: Arc::new(|| UpdateView::Hidden),
            check_now: Arc::new(|| {}),
            restart: Arc::new(|| Err(UpdateFailure::Busy)),
            take_ready_notice: Arc::new(|| None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_hidden_and_quiet() {
        let hooks = UpdateHooks::none();
        assert_eq!((hooks.state)(), UpdateView::Hidden);
        assert_eq!((hooks.take_ready_notice)(), None);
    }
}
