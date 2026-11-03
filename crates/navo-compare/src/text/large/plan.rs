use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use crate::model::Reason;
use crate::text::diff::diff_lines;
use crate::text::scan::{self, BUCKETS, Sample, ScanResult};

use super::batch::{Context, SideData};
use super::budget::Stall;
use super::budget::{self, Charge, Limits, Planned, Store, StoreSeed};
use super::read;
use super::schedule::Finished;
use super::schedule::{Schedule, group};
use super::{
    BATCH_LINES, DiffEvent, Driver, LargeSide, PROVISIONAL_BYTES, PROVISIONAL_LINES, Stage,
};

pub(crate) type Forge =
    Arc<dyn Fn(&mut Vec<scan::Candidate>, &mut Vec<scan::Candidate>) + Send + Sync>;
type BeforeDone = Arc<dyn Fn(&AtomicU64) + Send + Sync>;
pub(crate) type Hook = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Tuning {
    pub(crate) batch_lines: u64,
    pub(crate) forge: Option<Forge>,
    pub(crate) watchdog: Duration,
    pub(crate) stall: Option<Stall>,
    pub(crate) finished: Option<Finished>,
    pub(crate) expired: Option<Finished>,
    pub(crate) before_done: Option<BeforeDone>,
    pub(crate) before_hash: Option<Hook>,
    pub(crate) hold_available: bool,
    pub(crate) hold_all: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            batch_lines: BATCH_LINES,
            forge: None,
            watchdog: super::budget::WATCHDOG,
            stall: None,
            finished: None,
            expired: None,
            before_done: None,
            before_hash: None,
            hold_available: false,
            hold_all: false,
        }
    }
}

pub(super) type Scanned = (ScanResult, Charge);

/// One attempt's sides, batches and limits; `held` counts the batch slots.
pub(super) struct Attempt {
    pub(super) context: Arc<Context>,
    pub(super) schedule: Schedule,
    pub(super) limits: Arc<Limits>,
    pub(super) _held: Charge,
}

impl Driver {
    pub(super) fn plan(&self, left: Scanned, right: Scanned) -> Attempt {
        let ((mut left, left_held), (mut right, right_held)) = (left, right);
        let Planned {
            sections,
            terminal,
            mut charge,
        } = budget::plan_sections(
            &mut left,
            &mut right,
            self.tuning.forge.as_ref(),
            &self.ledger,
        );
        let mut held = left_held;
        held.absorb(right_held);
        let left = side_data(&self.left, left);
        let right = side_data(&self.right, right);
        held.settle((left.index.allocated_bytes() + right.index.allocated_bytes()) as u64);
        let context = Arc::new(Context {
            left,
            right,
            options: self.options,
            _charge: held,
        });
        let limits = Arc::new(self.limits());
        let count = sections.len();
        let entries = charge.take(Store::section_bytes().saturating_mul(count as u64));
        *self.store_slot() = Some(Store::new(
            Arc::clone(&context),
            entries,
            &limits,
            StoreSeed {
                sections: count,
                seq: Arc::clone(&self.seq),
            },
        ));
        let batches = group(sections, self.tuning.batch_lines, terminal, &mut charge);
        let schedule = Schedule::new(batches);
        charge.settle(schedule.bytes());
        Attempt {
            context,
            schedule,
            limits,
            _held: charge,
        }
    }

    fn limits(&self) -> Limits {
        Limits {
            ledger: Arc::clone(&self.ledger),
            focus: Arc::clone(&self.focus),
            paging: Arc::new(AtomicBool::new(false)),
            cancel: self.cancel.clone(),
            stall: self.tuning.stall.clone(),
            expired: self.tuning.expired.clone(),
            before_hash: self.tuning.before_hash.clone(),
            watchdog: self.tuning.watchdog,
            reach: budget::reach(self.tuning.batch_lines),
        }
    }

    pub(super) fn scan_phase(&self) -> Result<(Scanned, Scanned), Reason> {
        let total = self
            .left
            .source
            .len()
            .saturating_add(self.right.source.len());
        let meter = Meter {
            scanned: [AtomicU64::new(0), AtomicU64::new(0)],
            total,
        };
        thread::scope(|scope| {
            let left = scope.spawn(|| self.scan_side(&self.left, &meter, 0));
            let right = scope.spawn(|| self.scan_side(&self.right, &meter, 1));
            let provisional = self.provisional();
            let left = left.join().unwrap_or(Err(Reason::WorkerPanicked));
            let right = right.join().unwrap_or(Err(Reason::WorkerPanicked));
            provisional?;
            Ok((left?, right?))
        })
    }

    fn scan_side(&self, side: &LargeSide, meter: &Meter, slot: usize) -> Result<Scanned, Reason> {
        let cap = self.ledger.candidate_cap();
        let held = self
            .ledger
            .reserve(budget::bytes_of::<scan::Candidate>(cap))
            .unwrap_or_else(|| self.ledger.empty());
        let sample = Sample {
            rate: BUCKETS,
            cap: if held.bytes() > 0 { cap } else { 0 },
        };
        let mut progress = |at: u64| {
            meter.scanned[slot].store(at, Ordering::Relaxed);
            let [left, right] = &meter.scanned;
            let done = left.load(Ordering::Relaxed) + right.load(Ordering::Relaxed);
            self.hint(Stage::Scan, done, meter.total);
        };
        let source = side.source.as_ref();
        let scanned = scan::scan(
            source,
            side.layout,
            &self.options,
            sample,
            &self.cancel,
            &mut progress,
        )?;
        let mut held = held;
        held.settle(
            budget::bytes_of::<scan::Candidate>(scanned.candidates.capacity())
                .saturating_add(scanned.index.allocated_bytes() as u64),
        );
        Ok((scanned, held))
    }

    fn provisional(&self) -> Result<(), Reason> {
        let lines = PROVISIONAL_LINES * 2;
        let estimate = budget::work_estimate(lines, PROVISIONAL_BYTES * 2);
        let Some(mut held) = self.ledger.reserve(estimate) else {
            return Ok(());
        };
        let stop = || self.cancel.is_cancelled();
        let left = read::head(&self.left, PROVISIONAL_LINES, &stop)?;
        let right = read::head(&self.right, PROVISIONAL_LINES, &stop)?;
        let measured = budget::lines_bytes(&left).saturating_add(budget::lines_bytes(&right));
        held.cover_lines(lines, PROVISIONAL_BYTES * 2, measured);
        let left_set = left.set(self.left.layout.text.bom);
        let right_set = right.set(self.right.layout.text.bom);
        let hunks = diff_lines(left_set, right_set, &self.options).hunks;
        let Some((digests, charge)) = budget::hash_hunks(&left, &right, &hunks, &mut held)? else {
            return Ok(());
        };
        held.absorb(charge);
        self.send(DiffEvent::Provisional { hunks, digests })
    }
}

fn side_data(side: &LargeSide, scanned: ScanResult) -> SideData {
    SideData {
        source: Arc::clone(&side.source),
        layout: side.layout,
        len: scanned.index.scanned(),
        lines: scanned.lines,
        index: scanned.index,
    }
}

pub(super) struct Meter {
    scanned: [AtomicU64; 2],
    total: u64,
}
