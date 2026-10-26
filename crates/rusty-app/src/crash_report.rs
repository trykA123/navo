use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rusty_core::Location;
use rusty_ui::crash_notice::RecoveryContent;
use rusty_ui::window_view::CrashReportService;

use crate::crash::{self, Pending};
use crate::logging::Diagnostics;
use crate::startup::path_location;

pub(crate) struct CrashReport {
    pending: Option<Pending>,
    recovery: Option<crate::crash::store::RecoveryNotice>,
    started: Instant,
    diagnostics: Diagnostics,
    folder: Option<PathBuf>,
}

pub(crate) struct CrashStart {
    pub(crate) pending: Option<Pending>,
    pub(crate) started: Instant,
    pub(crate) diagnostics: Diagnostics,
    pub(crate) recovery: Option<crate::crash::store::RecoveryNotice>,
}

impl CrashStart {
    pub(crate) fn service(self) -> Option<Arc<dyn CrashReportService>> {
        if self.pending.is_none() && self.recovery.is_none() {
            return None;
        }
        let report = CrashReport {
            pending: self.pending,
            recovery: self.recovery,
            started: self.started,
            diagnostics: self.diagnostics,
            folder: crash::downloads_dir(),
        };
        Some(Arc::new(report))
    }
}

impl CrashReport {
    fn clear(&self) -> Result<(), String> {
        self.pending
            .as_ref()
            .map(crash::clear)
            .transpose()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl CrashReportService for CrashReport {
    fn save(&self) -> Result<Location, String> {
        let folder = self
            .folder
            .as_deref()
            .ok_or("No Downloads or home folder was found")?;
        let files = self
            .pending
            .as_ref()
            .map(|pending| pending.files.as_slice())
            .unwrap_or_default();
        let zip = crash::save_report_files(files, self.started, &self.diagnostics, folder)
            .map_err(|error| format!("The report was not saved: {error}"))?;
        self.clear()?;
        path_location(&zip)
            .ok_or_else(|| format!("Saved {}, but Navo cannot open that folder", zip.display()))
    }

    fn dismiss(&self) -> Result<(), String> {
        if let Some(recovery) = &self.recovery {
            recovery.dismiss()?;
        }
        self.clear()
    }

    fn recovery(&self) -> Option<RecoveryContent> {
        self.recovery.as_ref().map(|notice| notice.content())
    }

    fn folder(&self) -> Option<Location> {
        self.recovery.as_ref().and_then(|notice| notice.folder())
    }
}

#[cfg(test)]
mod tests;
