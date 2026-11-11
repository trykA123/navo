//! The large card runner: a real large diff drained into card-sized snapshots (D26.4, D26.7).

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{
    CARD_HUNKS, CardDetail, CardScan, DiffSink, DiffUpdate, LargeCard, LargeHunk, LargeProgress,
    LargeStage, PairControl, PairRequest,
};

use super::super::large::{LargeDiff, LargeInput};

/// How long the runner waits for one large update before it checks the cancel token.
const DETAIL_POLL: Duration = Duration::from_millis(50);

/// Run one large pair's scan, emitting card details until it is full, done or cancelled.
pub(crate) fn run(
    request: &PairRequest,
    input: LargeInput,
    cancel: &CancelToken,
    emit: &dyn Fn(CardDetail, bool),
) {
    let (send, updates) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = send.send(update);
    });
    let diff = LargeDiff::start(request.clone(), input, sink);
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match updates.recv_timeout(DETAIL_POLL) {
            Ok(DiffUpdate::Large {
                hunks,
                progress,
                ack,
                ..
            }) => {
                diff.acknowledge(ack);
                let card = large_card(&hunks, progress);
                let last = card.scan != CardScan::Running;
                emit(CardDetail::Large(card), last);
                if last {
                    return;
                }
            }
            Ok(DiffUpdate::Failed { text, .. }) => {
                emit(CardDetail::Failed(text), true);
                return;
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if !cancel.is_cancelled() {
                    emit(
                        CardDetail::Failed(navo_l10n::tr("compare-could-not-start")),
                        true,
                    );
                }
                return;
            }
        }
    }
}

/// The card body of a large pair: at most `CARD_HUNKS` hunks and how far its scan got.
pub(crate) fn large_card(hunks: &[LargeHunk], progress: Option<LargeProgress>) -> LargeCard {
    let mut sorted = hunks.to_vec();
    sorted.sort_by_key(|hunk| (hunk.left_rows.start, hunk.right_rows.start));
    let found = u32::try_from(hunks.len()).unwrap_or(u32::MAX);
    sorted.truncate(CARD_HUNKS);
    let finished = progress.is_some_and(|progress| {
        progress.stage == LargeStage::Sections
            && progress.total > 0
            && progress.done >= progress.total
    });
    let scan = if finished {
        CardScan::Done
    } else if sorted.len() >= CARD_HUNKS {
        CardScan::Full
    } else {
        CardScan::Running
    };
    LargeCard {
        hunks: sorted,
        found,
        progress,
        scan,
    }
}
