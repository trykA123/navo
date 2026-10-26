use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rusty_core::Location;
use rusty_ui::window_view::CrashReportService;

use crate::crash::{self, Pending};
use crate::logging::Diagnostics;
use crate::startup::path_location;

pub(crate) struct CrashReport {
    pending: Pending,
    started: Instant,
    diagnostics: Diagnostics,
    folder: Option<PathBuf>,
}

pub(crate) struct CrashStart {
    pub(crate) pending: Option<Pending>,
    pub(crate) started: Instant,
    pub(crate) diagnostics: Diagnostics,
}

impl CrashStart {
    pub(crate) fn service(self) -> Option<Arc<dyn CrashReportService>> {
        let report = CrashReport {
            pending: self.pending?,
            started: self.started,
            diagnostics: self.diagnostics,
            folder: crash::downloads_dir(),
        };
        Some(Arc::new(report))
    }
}

impl CrashReport {
    fn clear(&self) {
        if let Err(error) = crash::clear(&self.pending) {
            tracing::warn!(%error, "cannot clear the pending crash");
        }
    }
}

impl CrashReportService for CrashReport {
    fn save(&self) -> Result<Location, String> {
        let folder = self
            .folder
            .as_deref()
            .ok_or("No Downloads or home folder was found")?;
        let zip = crash::save_report(&self.pending, self.started, &self.diagnostics, folder)
            .map_err(|error| format!("The report was not saved: {error}"))?;
        self.clear();
        path_location(&zip)
            .ok_or_else(|| format!("Saved {}, but Navo cannot open that folder", zip.display()))
    }

    fn dismiss(&self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests;
