use rusty_core::Location;
use rusty_core::retention::RetentionId;
use rusty_l10n::t;

use super::FerriteWindow;
use crate::browser::{RetainedHooks, RetireUndo, UndoOffer};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct RetainedUndo {
    pub(super) offer: UndoOffer,
    pub(super) ids: Vec<RetentionId>,
    pub(super) retire: RetireUndo,
}

struct Restored {
    total: usize,
    places: Vec<Location>,
    failures: Vec<String>,
}

fn restore_all(undo: &RetainedUndo, hooks: &dyn RetainedHooks) -> Result<Restored, String> {
    (undo.retire)(&undo.offer)?;
    let mut restored = Restored {
        total: undo.ids.len(),
        places: Vec::new(),
        failures: Vec::new(),
    };
    for id in &undo.ids {
        match hooks.restore(*id) {
            Ok(place) => restored.places.push(place),
            Err(reason) => restored.failures.push(reason),
        }
    }
    tracing::info!(
        job = undo.offer.job,
        restored = restored.places.len(),
        failed = restored.failures.len(),
        "retained undo finished"
    );
    Ok(restored)
}

fn notice(restored: &Result<Restored, String>) -> StatusNotice {
    let (text, is_error) = match restored {
        Err(reason) => (
            t!("undo-retained-start-failed", reason = reason.clone()),
            true,
        ),
        Ok(done) => match done.failures.first() {
            None => (t!("undo-retained-done", count = done.total), false),
            Some(reason) if done.places.is_empty() => (
                t!(
                    "undo-retained-none",
                    total = done.total,
                    reason = reason.clone()
                ),
                true,
            ),
            Some(reason) => (
                t!(
                    "undo-retained-partial",
                    restored = done.places.len(),
                    total = done.total,
                    failed = done.failures.len(),
                    reason = reason.clone()
                ),
                true,
            ),
        },
    };
    StatusNotice {
        text: text.into(),
        is_error,
    }
}

fn parents(places: &[Location]) -> Vec<Location> {
    let mut folders: Vec<Location> = Vec::new();
    for folder in places.iter().filter_map(Location::parent) {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

impl FerriteWindow {
    pub(super) fn undo_retained(&mut self, undo: RetainedUndo, cx: &mut Context<Self>) {
        self.browser.invalidate_undo();
        self.job_ui.is_restoring = true;
        let hooks = self.browser.retained_hooks();
        let restoring = cx.background_spawn(async move { restore_all(&undo, hooks.as_ref()) });
        cx.spawn(async move |this, cx| {
            let restored = restoring.await;
            let _ = this.update(cx, |view, cx| view.retained_restored(&restored, cx));
        })
        .detach();
    }

    fn retained_restored(&mut self, restored: &Result<Restored, String>, cx: &mut Context<Self>) {
        self.browser.invalidate_undo();
        self.job_ui.is_restoring = false;
        if let Ok(done) = restored {
            self.browser.refresh_unwatched(&parents(&done.places));
        }
        self.show_notice(notice(restored));
        cx.notify();
    }
}
