use std::fs;
use std::path::Path;
use std::sync::Arc;

use navo_clipboard::{ClipError, ClipService, ClipStore, Limits, Opened};
use navo_shell::ClipboardError;

use super::capture::deliver;
use super::listeners::Listeners;
use super::{Backends, StartWatch};

/// The open store, its service and the watcher feeding it.
pub(super) struct Running {
    service: Arc<ClipService>,
    notice: Option<String>,
    watcher: Option<Box<dyn Send>>,
    /// The watcher failed to start (not merely unsupported), so nothing is recorded.
    watch_failed: bool,
}

impl Running {
    pub(super) fn start(
        db: &Path,
        record: bool,
        backends: &Backends,
        listeners: &Arc<Listeners>,
    ) -> Result<Self, ClipError> {
        if let Some(folder) = db.parent() {
            fs::create_dir_all(folder)?;
        }
        let store = ClipStore::open(db, Limits::default())?;
        let notice = match store.opened() {
            Opened::Clean => None,
            Opened::MovedAside(_) => {
                tracing::warn!("the clipboard history file was damaged and set aside");
                Some(navo_l10n::tr("clipboard-moved-aside"))
            }
        };
        let service = Arc::new(ClipService::start(
            store,
            Limits::default(),
            backends.ports.clone(),
        ));
        service.set_recording(record);
        let heard = listeners.clone();
        service.subscribe(Box::new(move |_| heard.notify()));
        let (watcher, watch_failed) = watch(&service, &backends.watch);
        Ok(Self {
            service,
            notice,
            watcher,
            watch_failed,
        })
    }

    pub(super) fn service(&self) -> &Arc<ClipService> {
        &self.service
    }

    /// What the palette should say: a damaged file first, then a failed recorder (a watcher
    /// that did not start or a worker that died), then a pause.
    pub(super) fn notice(&self) -> Option<String> {
        self.notice
            .clone()
            .or_else(|| self.is_failed().then(failed_notice))
            .or_else(|| (!self.service.is_recording()).then(|| navo_l10n::tr("clipboard-paused")))
    }

    pub(super) fn is_failed(&self) -> bool {
        self.watch_failed || !self.service.is_alive()
    }

    /// Stops the watcher first, so nothing new arrives, then drains the service.
    pub(super) fn stop(self) {
        drop(self.watcher);
        if !self.service.stop() {
            tracing::warn!("the clipboard service did not stop in time");
        }
    }
}

pub(super) fn failed_notice() -> String {
    navo_l10n::tr("clipboard-recording-failed")
}

/// The watcher guard and whether the start failed.
fn watch(service: &Arc<ClipService>, start: &StartWatch) -> (Option<Box<dyn Send>>, bool) {
    let target = service.clone();
    match start(Box::new(move |capture| deliver(&target, capture))) {
        Ok(watcher) => (Some(watcher), false),
        Err(ClipboardError::Unsupported) => {
            tracing::debug!("the clipboard watcher is not available here");
            (None, false)
        }
        Err(error) => {
            tracing::warn!(
                kind = error_kind(&error),
                "the clipboard watcher did not start"
            );
            (None, true)
        }
    }
}

fn error_kind(error: &ClipboardError) -> &'static str {
    match error {
        ClipboardError::Busy => "busy",
        ClipboardError::Unsupported => "unsupported",
        ClipboardError::Malformed => "malformed",
        ClipboardError::Os(_) => "os",
    }
}
