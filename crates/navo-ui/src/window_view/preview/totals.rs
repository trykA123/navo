use std::sync::Arc;

use navo_core::Location;
use navo_fs::{FolderSize, JobSlot, OwnerGuard, Priority, volume_of};

use super::Preview;
use super::folder::FolderPreview;
use super::inbox::Arrival;
use super::scan::Phase;
use crate::browser::{FolderTotals, TotalsState};
use crate::window_view::NavoWindow;

impl Preview {
    pub(super) fn ask_totals(&self, load: u64, folder: Location) -> OwnerGuard {
        let owner = self.reads.owner();
        let slot = JobSlot {
            volume: volume_of(&folder),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        let (send, wake) = (self.inbox.send.clone(), Arc::clone(&self.inbox.wake));
        let query = Arc::clone(&self.totals);
        self.reads.submit(
            slot,
            Box::new(move |_| {
                let TotalsState::Ready(totals) = query.query(&folder) else {
                    return;
                };
                if send.send(Arrival::Totals { load, totals }).is_ok() {
                    wake();
                }
            }),
        );
        owner
    }
}

impl FolderPreview {
    fn is_walked(&self) -> bool {
        matches!(self.scan.phase, Phase::Done { .. })
    }

    /// Whether this load waits for index totals instead of showing a number.
    #[expect(dead_code, reason = "T14 stores the load's Busy decision here")]
    pub(super) fn waiting_for_totals(&self) -> bool {
        false
    }

    pub(super) fn shown_totals(&self) -> (FolderSize, bool) {
        let walked = self
            .scan
            .profile
            .as_ref()
            .map(|profile| profile.totals)
            .unwrap_or_default();
        match &self.index {
            Some(index) if !self.is_walked() => {
                let totals = FolderSize {
                    files: index.files.into(),
                    folders: index.folders.into(),
                    skipped: walked.skipped,
                    ..FolderSize::apparent_only(index.bytes)
                };
                (totals, index.complete)
            }
            _ => (walked, self.is_walked()),
        }
    }
}

impl NavoWindow {
    pub(super) fn totals_arrived(&mut self, load: u64, totals: FolderTotals) -> bool {
        let Some(folder) = self
            .preview
            .folder_mut()
            .filter(|folder| folder.load == load && !folder.is_walked())
        else {
            return false;
        };
        let target = totals.complete.then(|| folder.target.clone());
        let bytes = totals.bytes;
        folder.index = Some(totals);
        if let Some(target) = target {
            self.show_size(&target, FolderSize::apparent_only(bytes));
        }
        true
    }
}
