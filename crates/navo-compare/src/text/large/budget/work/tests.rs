use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use navo_core::paging::LineIndex;

use crate::model::{Budget, CancelToken};
use crate::text::large::tests::side;

use super::super::super::batch::{Batch, Context, Env, Sections, SideData};
use super::super::{Ledger, Limits, Out, WATCHDOG, work_estimate};
use super::{Read, ReadDiff, coarse_batch, detail, read_diff};
use crate::anchor::SectionPair;
use crate::text::large::schedule::Beat;

fn context(ledger: &Arc<Ledger>) -> Context {
    let large = side(b"a\nb\nc\nd\n");
    let data = || SideData {
        source: Arc::clone(&large.source),
        layout: large.layout,
        len: 8,
        lines: 4,
        index: LineIndex::new(),
    };
    Context {
        left: data(),
        right: data(),
        options: Default::default(),
        _charge: ledger.empty(),
    }
}

#[test]
fn recovery_emits_one_block_when_the_ledger_is_full() {
    let budget = Out::block_bytes();
    let ledger = Arc::new(Ledger::new(Budget::from_bytes(budget)));
    let limits = Limits {
        ledger: Arc::clone(&ledger),
        focus: Arc::new(AtomicU64::new(0)),
        paging: Arc::new(AtomicBool::new(false)),
        cancel: CancelToken::new(),
        stall: None,
        expired: None,
        before_hash: None,
        watchdog: WATCHDOG,
        reach: 0,
    };
    let list = (0..4)
        .map(|at| crate::anchor::SectionPair {
            left: at..at + 1,
            right: at..at + 1,
        })
        .collect();
    let batch = Batch {
        sections: Arc::new(Sections {
            list,
            charge: ledger.empty(),
        }),
        start_anchor: false,
        end_anchor: false,
        terminal: false,
    };
    let beat = Beat::default();
    let forced = Vec::new();
    let stop = || false;
    let env = Env {
        context: &context(&ledger),
        limits: &limits,
        stop: &stop,
        beat: &beat,
        forced: &forced,
    };
    let (out, sections) = coarse_batch(&env, &batch)
        .expect("recovery reads the batch")
        .expect("one fallback block fits");
    assert_eq!(sections, 4);
    assert_eq!(out.parts.len(), 1);
    assert!(ledger.peak() <= budget);
}

#[test]
fn the_raw_bytes_are_released_once_the_digests_exist() {
    const LEN: usize = 64 * 1024;
    let data = |byte: u8| {
        let large = side(&vec![byte; LEN]);
        SideData {
            source: Arc::clone(&large.source),
            layout: large.layout,
            len: LEN as u64,
            lines: 1,
            index: LineIndex::new(),
        }
    };
    let ledger = Arc::new(Ledger::new(Budget::default()));
    let context = Context {
        left: data(b'a'),
        right: data(b'b'),
        options: Default::default(),
        _charge: ledger.empty(),
    };
    let pair = SectionPair {
        left: 0..1,
        right: 0..1,
    };
    let stop = || false;
    let detail = detail(&context, &pair, &stop).unwrap();
    let estimate = work_estimate(2, 2 * LEN as u64);
    let mut charge = ledger.reserve(estimate).unwrap();
    let read = Read {
        context: &context,
        detail: &detail,
        charge: &mut charge,
        stop: &stop,
        before_hash: None,
    };
    let ReadDiff::Hunks(hunks) = read_diff(read).unwrap() else {
        panic!("the section diffs");
    };
    assert!(hunks.charge.bytes() > 0);
    assert!(
        charge.bytes() + 2 * LEN as u64 <= estimate,
        "{} bytes held of {estimate} reserved",
        charge.bytes()
    );
}
