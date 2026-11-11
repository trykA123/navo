//! What the file diff shows now and which worker request an update belongs to.

use std::sync::Arc;

use crate::prelude::SharedString;

use crate::compare_diff::{DiffUpdate, PairKind, PairText};

/// What the file diff shows now.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::window_view) enum FileState {
    Loading,
    Opened {
        kind: PairKind,
        text: Option<Arc<PairText>>,
    },
    Failed(SharedString),
}

/// The worker request id an update belongs to.
pub(super) fn request_of(update: &DiffUpdate) -> u64 {
    match update {
        DiffUpdate::Opened { request, .. }
        | DiffUpdate::Spans { request, .. }
        | DiffUpdate::Large { request, .. }
        | DiffUpdate::LargeLines { request, .. }
        | DiffUpdate::Bytes { request, .. }
        | DiffUpdate::Image { request, .. }
        | DiffUpdate::ChangedOnDisk { request }
        | DiffUpdate::Failed { request, .. } => *request,
    }
}
