use std::sync::Arc;

use crate::anchor::SectionPair;
use crate::model::Budget;
use crate::text::diff::Hunk;
use crate::text::large::batch::{Context, SideData};
use crate::text::large::tests::side;
use navo_core::paging::LineIndex;

use super::super::budget::{Ledger, Out, Part, Store, StoreSeed, WATCHDOG, bytes_of};
use super::*;

fn context(ledger: &Arc<Ledger>) -> Arc<Context> {
    let side = side(b"a\n");
    let data = || SideData {
        source: Arc::clone(&side.source),
        layout: side.layout,
        len: 2,
        lines: 1,
        index: LineIndex::new(),
    };
    Arc::new(Context {
        left: data(),
        right: data(),
        options: Default::default(),
        _charge: ledger.empty(),
    })
}

fn limits(ledger: &Arc<Ledger>) -> Limits {
    Limits {
        ledger: Arc::clone(ledger),
        focus: Arc::default(),
        paging: Arc::default(),
        cancel: crate::model::CancelToken::new(),
        stall: None,
        expired: None,
        watchdog: WATCHDOG,
        reach: 0,
    }
}

fn batch_out(ledger: &Arc<Ledger>) -> Out {
    let allocation = bytes_of::<Hunk>(4) + bytes_of::<Part>(1);
    let mut out = Out::new(ledger.reserve(allocation).unwrap());
    let hunk = Hunk {
        left_lines: 0..1,
        right_lines: 0..1,
        left_bytes: 0..2,
        right_bytes: 0..2,
    };
    out.hunks = vec![hunk; 4];
    out.parts = vec![Part {
        pair: SectionPair {
            left: 0..1,
            right: 0..1,
        },
        end: 4,
        kind: Kind::Detail,
    }];
    out
}

#[test]
fn covering_a_batch_keeps_its_reservation_until_recording() {
    let budget = 4096;
    let ledger = Arc::new(Ledger::new(Budget::from_bytes(budget)));
    let section = ledger.reserve(Store::section_bytes()).unwrap();
    let seed = StoreSeed {
        sections: 1,
        seq: Arc::default(),
    };
    let mut store = Store::new(context(&ledger), section, &limits(&ledger), seed);
    let out = batch_out(&ledger);
    let held = ledger.reserve(ledger.available()).unwrap();
    let (covered, _steps, parts, charge) = cover(out, 0..1, 0..1);
    assert!(
        ledger.reserve(1).is_none(),
        "the competing worker cannot use live hunk bytes"
    );
    let _recorded = store.record(&parts, 0, (covered.hunks.capacity(), charge));
    assert!(
        ledger.peak() <= budget,
        "recording only settles the charge down"
    );
    drop(store);
    drop(held);
    drop(covered);
    assert_eq!(ledger.used(), 0);
}
