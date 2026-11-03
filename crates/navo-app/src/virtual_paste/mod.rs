//! Paste of virtual files (D104): the descriptor gives a copy job its items, and the clipboard
//! reader becomes the job's source.

mod items;
mod mark;
mod source;

use std::sync::{Arc, PoisonError};

use navo_core::{Location, ProviderId};
use navo_fs::Sources;
use navo_shell::{VirtualClipboard, VirtualError};
use navo_ui::browser::StreamPaste;

use source::{LastError, VirtualFiles};

const PROVIDER: &str = "pasted";

/// Builds the paste job's input. The clipboard reader lives as long as the job may read.
pub(crate) fn stream_paste(clipboard: VirtualClipboard) -> Result<StreamPaste, String> {
    let provider = ProviderId::new(PROVIDER).map_err(|error| error.to_string())?;
    let planned = items::plan_items(
        clipboard.entries(),
        &Location::provider_root(provider.clone()),
    );
    let last_error = LastError::default();
    let explained = Arc::clone(&last_error);
    let files = VirtualFiles {
        provider,
        mark: mark::for_source(clipboard.source_kind(), clipboard.archive()),
        clipboard,
        files: planned.files,
        last_error,
    };
    Ok(StreamPaste {
        sources: Sources::new().with(Arc::new(files)),
        items: planned.items,
        refused: planned.refused,
        items_only: planned.items_only,
        explain: Some(Arc::new(move || {
            explained
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        })),
    })
}

/// The text shown when the file list itself cannot be read.
pub(crate) fn list_error(error: &VirtualError) -> String {
    match error {
        VirtualError::Stalled => navo_l10n::tr("paste-virtual-list-stalled"),
        VirtualError::Malformed => navo_l10n::tr("paste-virtual-damaged"),
        VirtualError::Cancelled | VirtualError::Source(_) => {
            navo_l10n::t!("paste-virtual-list-failed", error = error.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
