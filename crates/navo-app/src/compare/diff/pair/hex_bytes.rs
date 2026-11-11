//! The byte diff a hex body asks the pair worker for (V39): start, replace and clear it.

use std::sync::PoisonError;

use navo_compare::bytes::Align;
use navo_ui::compare_diff::{HexAlign, PairRequest};

use super::super::bytes::{ByteDiff, ByteInput};
use super::super::load::LoadInput;
use super::{Commands, Worker};

/// Start or replace the byte diff of a binary pair for `align`; a text pair ignores it.
/// It also stops the engine's own byte diff, which only fills the unrequested run 0.
pub(super) fn start_bytes(
    (request, input, id): (&PairRequest, &LoadInput, u64),
    (align, run): (HexAlign, u64),
    worker: &Worker<'_>,
) {
    let (Some(left), Some(right)) = (input.left.clone(), input.right.clone()) else {
        return;
    };
    let align = match align {
        HexAlign::Offset => Align::Offset,
        HexAlign::Content => Align::Content,
    };
    let input = ByteInput {
        left,
        right,
        align,
        run,
        budget: worker.plan.budget,
    };
    let mut request = request.clone();
    request.id = id;
    worker.live.clear();
    let running = ByteDiff::start(request, input, worker.sink.clone());
    *worker
        .commands
        .bytes
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(running);
}

/// Drop the running byte diff; its workers cancel.
pub(super) fn clear_bytes(commands: &Commands) {
    commands
        .bytes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
}
