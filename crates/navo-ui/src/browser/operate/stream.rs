use std::fmt;
use std::sync::Arc;

use navo_core::{Location, OpKind, ScannedItem, is_protected};
use navo_fs::{Progress, Sources};

use super::paste::PasteRefusal;
use super::{Browser, OpRequest};

/// The source's own text for the failure a job ended with, if it has one.
pub type Explain = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// A pasted item whose name was refused; nothing is written for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub name: String,
    pub reason: String,
}

/// Files another program offers without paths (virtual files). Always a copy: the source is
/// never changed.
#[derive(Clone)]
pub struct StreamPaste {
    /// Has the reader for the provider of `items`.
    pub sources: Sources,
    /// Pre-order, depth 0 for top items, on the source provider.
    pub items: Vec<ScannedItem>,
    pub refused: Vec<Refused>,
    /// Progress counts items only, because some sizes are unknown.
    pub items_only: bool,
    pub explain: Option<Explain>,
}

impl fmt::Debug for StreamPaste {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamPaste")
            .field("items", &self.items)
            .field("refused", &self.refused)
            .field("items_only", &self.items_only)
            .finish_non_exhaustive()
    }
}

impl StreamPaste {
    pub(super) fn tops(&self) -> Vec<Location> {
        self.items
            .iter()
            .filter(|item| item.depth == 0)
            .map(|item| item.location.clone())
            .collect()
    }

    /// Hides byte counts while some sizes are unknown.
    pub(super) fn shown(&self, progress: &Progress) -> Progress {
        if !self.items_only {
            return progress.clone();
        }
        Progress {
            bytes_done: 0,
            bytes_total: 0,
            ..progress.clone()
        }
    }
}

impl Browser {
    /// Plans a copy of `paste` into `target`. Conflicts are asked before any byte is read.
    pub fn paste_stream(
        &mut self,
        paste: StreamPaste,
        target: Location,
    ) -> Result<u64, PasteRefusal> {
        if crate::browser::find::is_results(&target) {
            return Err(PasteRefusal::IntoResults);
        }
        if paste.items.is_empty() && paste.refused.is_empty() {
            return Err(PasteRefusal::Nothing);
        }
        self.next_op += 1;
        let request = OpRequest {
            id: self.next_op,
            kind: OpKind::Copy,
            sources: paste.tops(),
            asks: is_protected(&target, &self.operations.protected),
            target: Some(target),
            same_volume: false,
            protected: self.operations.protected.clone(),
            source_tab: self.state.active_tab().id(),
            structure: None,
            stream: Some(paste),
        };
        let id = request.id;
        if self.submit_plan(request) {
            Ok(id)
        } else {
            Err(PasteRefusal::Nothing)
        }
    }
}
