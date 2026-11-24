//! Archives in the pane: routes archive locations to `ArchiveFs`, opens an archive from its
//! local file, leaves an archive root back to the folder that holds it and notes unsafe entries.
//! Archives are read-only: only a copy out of one is planned, as a streamed copy.

use std::sync::Arc;

use gpui_kit::SharedString;
use navo_core::{Command, Location, OpKind, PaneId};
use navo_fs::archive::{self, ArchiveFs, UnsafeReason};
use navo_fs::{ProviderOps, Sources};
use navo_l10n::t;

use super::Browser;
use super::operate::{PasteRefusal, StreamPaste};

/// True when `location` lies inside an archive.
pub(super) fn is_inside(location: &Location) -> bool {
    location.provider() == &archive::provider_id()
}

/// Whether a `kind` operation from `sources` into `target` writes into or out of an archive.
/// Only a copy out of an archive reads it, so every other such operation is refused.
pub(super) fn writes_archive(
    kind: OpKind,
    sources: &[Location],
    target: Option<&Location>,
) -> bool {
    target.is_some_and(is_inside) || (kind != OpKind::Copy && sources.iter().any(is_inside))
}

/// Why an archive entry named `name` is unsafe to unpack, as row text; None for a safe name.
fn unsafe_note(name: &str) -> Option<SharedString> {
    let text = match archive::unsafe_name(name)? {
        UnsafeReason::Parent => t!("archive-unsafe-parent"),
        UnsafeReason::Absolute => t!("archive-unsafe-absolute"),
        UnsafeReason::Drive => t!("archive-unsafe-drive"),
        UnsafeReason::Nul => t!("archive-unsafe-nul"),
        UnsafeReason::WindowsName => t!("archive-unsafe-windows"),
    };
    Some(text.into())
}

/// A shared archive reader for the browser; it caches archive indexes between listings.
pub(super) fn reader() -> Arc<ArchiveFs> {
    Arc::new(ArchiveFs::new())
}

impl Browser {
    /// The provider that lists and reads `location`: `ArchiveFs` for archive locations, the
    /// browser's own provider for every other one.
    pub(super) fn provider_for(&self, location: &Location) -> Arc<dyn ProviderOps> {
        if is_inside(location) {
            let reader: Arc<dyn ProviderOps> = Arc::clone(&self.archives) as _;
            return reader;
        }
        Arc::clone(&self.provider)
    }

    /// Opens the local archive `file` as a folder in the active tab. False when the name is not a
    /// supported archive or the tab refused the move, so the caller launches the file instead.
    pub(super) fn open_archive(&mut self, file: &Location) -> bool {
        let Some(location) = archive::archive_root(file) else {
            return false;
        };
        self.command(&Command::PaneNavigate { location }).is_ok()
    }

    /// Leaves an archive root for the folder holding the archive, with the cursor on the archive.
    /// None when the active tab is not at an archive root; otherwise whether the move worked.
    pub(super) fn leave_archive(&mut self) -> Option<bool> {
        let (folder, name) = archive::leave_archive(self.state.active_tab().location())?;
        Some(self.open_at(folder, name))
    }

    /// Refuses a write in the active tab (new folder, rename, paste) while it shows an archive.
    #[must_use]
    pub fn archive_refusal(&self) -> Option<PasteRefusal> {
        let here = self.state.active_tab().location();
        is_inside(here).then_some(PasteRefusal::ReadOnlyArchive)
    }

    /// The streamed copy for `sources` inside an archive: the shared reader serves the bytes and
    /// the planner lists the entries first. None for sources outside archives.
    pub(super) fn archive_stream(&self, sources: &[Location]) -> Option<StreamPaste> {
        sources.first().filter(|source| is_inside(source))?;
        let reader: Arc<dyn navo_fs::SourceOps> = Arc::clone(&self.archives) as _;
        Some(StreamPaste {
            sources: Sources::new().with(reader),
            items: Vec::new(),
            refused: Vec::new(),
            items_only: false,
            explain: None,
        })
    }

    /// The row notes for `pane`'s list: unsafe-entry reasons inside an archive, none elsewhere.
    pub(crate) fn row_notes(&self, pane: PaneId) -> Option<crate::file_list::NoteFn> {
        let location = self.state.pane(pane)?.active().location();
        let note: crate::file_list::NoteFn = unsafe_note;
        is_inside(location).then_some(note)
    }
}
