//! The binary card runner: a real byte diff drained into the first changed window (D26.4, D26.8).

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use navo_compare::model::CancelToken;
use navo_core::Location;
use navo_core::paging::{HEX_WIDTH, hex_row_start, read_full};
use navo_ui::compare_diff::{
    ByteProgress, ByteRegionView, BytesCard, CardDetail, CardScan, DiffSink, DiffUpdate,
    HEX_CARD_ROWS, HexWindow, PairRequest,
};

use super::super::bytes::{ByteDiff, ByteInput};
use super::super::sources::Sources;
use crate::compare::settings::CompareSettings;

/// How long the runner waits for one byte update before it checks the cancel token.
const DETAIL_POLL: Duration = Duration::from_millis(50);

/// Bytes one side of a card window holds: nine hex rows of sixteen.
const WINDOW_BYTES: usize = HEX_CARD_ROWS * 16;

/// Everything one binary card job needs: the pair, its sides, spool sources and settings.
pub(crate) struct BytesJob {
    pub request: PairRequest,
    pub input: ByteInput,
    pub sources: Sources,
    pub settings: CompareSettings,
}

/// Run one binary pair's first-region scan, emitting card details until full, done or cancelled.
pub(crate) fn run(job: BytesJob, cancel: &CancelToken, emit: &dyn Fn(CardDetail, bool)) {
    let wide = (
        job.input.left.1.size > u64::from(u32::MAX),
        job.input.right.1.size > u64::from(u32::MAX),
    );
    let (send, updates) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = send.send(update);
    });
    let _diff = ByteDiff::start(job.request.clone(), job.input, sink);
    let ctx = CardCtx {
        reader: Reader {
            sources: &job.sources,
            settings: &job.settings,
            cancel,
        },
        request: &job.request,
        wide,
    };
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match updates.recv_timeout(DETAIL_POLL) {
            Ok(DiffUpdate::Bytes {
                regions, progress, ..
            }) => {
                if ctx.apply(&regions, progress, emit) {
                    return;
                }
            }
            Ok(DiffUpdate::Failed { text, .. }) => {
                emit(CardDetail::Failed(text), true);
                return;
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The first window of a card: four hex rows above `offset`'s row, never below zero.
pub(crate) fn window_start(offset: u64) -> u64 {
    hex_row_start(offset).saturating_sub(4 * HEX_WIDTH)
}

/// One job's card context: the window reader, its pair and each side's offset width.
struct CardCtx<'a> {
    reader: Reader<'a>,
    request: &'a PairRequest,
    wide: (bool, bool),
}

impl CardCtx<'_> {
    /// Turn one `Bytes` snapshot into a card; true when the job has its last detail.
    fn apply(
        &self,
        regions: &[ByteRegionView],
        progress: Option<ByteProgress>,
        emit: &dyn Fn(CardDetail, bool),
    ) -> bool {
        if let Some(region) = first_region(regions) {
            return match self.window_card(region, progress) {
                Ok(card) => {
                    emit(CardDetail::Bytes(card), true);
                    true
                }
                Err(_) => {
                    emit(
                        CardDetail::Failed(navo_l10n::tr("compare-could-not-start")),
                        true,
                    );
                    true
                }
            };
        }
        let done = progress.is_some_and(|progress| progress.done == progress.total);
        let card = BytesCard {
            window: None,
            progress,
            scan: if done {
                CardScan::Done
            } else {
                CardScan::Running
            },
        };
        emit(CardDetail::Bytes(card), done);
        done
    }

    /// The card window around one region, read fresh from both sides.
    fn window_card(
        &self,
        region: &ByteRegionView,
        progress: Option<ByteProgress>,
    ) -> Result<BytesCard, String> {
        let (Some(left), Some(right)) = (self.request.left.as_ref(), self.request.right.as_ref())
        else {
            return Err("a binary pair needs both locations".to_owned());
        };
        let left_start = window_start(region.left.start);
        let right_start = window_start(region.right.start);
        let window = HexWindow {
            left_start,
            right_start,
            left: self.reader.read_side(left, left_start)?,
            right: self.reader.read_side(right, right_start)?,
            changed_left: region.left.clone(),
            changed_right: region.right.clone(),
            wide: self.wide,
        };
        Ok(BytesCard {
            window: Some(window),
            progress,
            scan: CardScan::Full,
        })
    }
}

/// One side of a card window: a fresh source over the job's spool and the job's cancel token.
struct Reader<'a> {
    sources: &'a Sources,
    settings: &'a CompareSettings,
    cancel: &'a CancelToken,
}

impl Reader<'_> {
    /// Read up to `HEX_CARD_ROWS` rows from `start`; a shorter side stops at its end.
    fn read_side(&self, location: &Location, start: u64) -> Result<Vec<u8>, String> {
        let source = self
            .sources
            .open(location, self.settings, self.cancel)
            .map_err(|reason| navo_l10n::tr(reason.fluent_id().as_str()))?;
        let mut bytes = vec![0u8; WINDOW_BYTES];
        let read = read_full(source.as_ref(), start, &mut bytes)?;
        bytes.truncate(read);
        Ok(bytes)
    }
}

/// The first region of a snapshot: smallest left start, then smallest right start.
fn first_region(regions: &[ByteRegionView]) -> Option<&ByteRegionView> {
    regions
        .iter()
        .min_by_key(|region| (region.left.start, region.right.start))
}
