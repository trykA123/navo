use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use navo_core::paging::LineIndex;

use crate::model::{Budget, CancelToken};
use crate::text::large::tests::{collect, side};
use crate::text::large::{
    Covered, DiffEvent, Driver, EVENT_BUFFER, Progress, Stage, Summary, Tuning,
};

use super::super::{Kind, Ledger, Part};
use super::*;
mod finish;
use crate::text::large::batch::{Context, SideData};

fn seq() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

fn limits(ledger: &Arc<Ledger>, paging: bool) -> Limits {
    Limits {
        ledger: Arc::clone(ledger),
        focus: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        paging: Arc::new(AtomicBool::new(paging)),
        cancel: CancelToken::new(),
        stall: None,
        expired: None,
        watchdog: Duration::from_secs(1),
        reach: 0,
    }
}

fn context(ledger: &Arc<Ledger>) -> Context {
    let large = side(b"a\n");
    let data = || SideData {
        source: Arc::clone(&large.source),
        layout: large.layout,
        len: 2,
        lines: 1,
        index: LineIndex::new(),
    };
    Context {
        left: data(),
        right: data(),
        options: Default::default(),
        _charge: ledger.empty(),
    }
}

fn part(end: usize, kind: Kind) -> Part {
    Part {
        pair: SectionPair {
            left: 0..1,
            right: 0..1,
        },
        end,
        kind,
    }
}

#[test]
fn metadata_failure_reports_counts_only_without_storing_sections() {
    let ledger = Arc::new(Ledger::new(Budget::from_bytes(
        bytes_of::<Entry>(2).saturating_add(1),
    )));
    let limits = limits(&ledger, false);
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let parts = [part(1, Kind::Detail), part(2, Kind::Coarse)];
    let charge = ledger.reserve(HUNK_SIZE).unwrap();
    let recorded = store.record(&parts, 0, (1, charge));
    assert!(matches!(recorded, Recorded::CountsOnly));
    assert!(store.find(&(0..1)).is_none(), "no metadata was stored");
    assert_eq!(ledger.used(), 0, "counts-only retains no batch charge");
}

#[test]
fn an_eviction_charge_lasts_until_the_view_acknowledges_it() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let limits = limits(&ledger, true);
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let parts = [part(4, Kind::Detail)];
    let Recorded::Evictions(evictions) = store.record(&parts, u64::MAX, (0, ledger.empty())) else {
        panic!("the eviction event fits");
    };
    let held = ledger.used();
    assert_eq!(evictions.paged().len(), 1);
    assert_eq!(evictions.paged()[0].state, PagedState::Rediffable);
    assert_eq!(ledger.used(), held, "the charge waits for application");
    evictions.acknowledge();
    assert!(ledger.used() < held, "acknowledgement frees the hunks");
}

#[test]
fn acknowledgement_follows_paged_event_sequence() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let limits = limits(&ledger, false);
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let first = ledger.reserve(64).expect("first hold fits");
    let first = Evictions {
        paged: vec![event()],
        hold: first,
    };
    let (_, first_seq) = store.defer(first);
    assert_eq!(first_seq, 1);
    let held = ledger.used();
    store.acknowledge(0);
    assert_eq!(ledger.used(), held);
    store.acknowledge(1);
    assert!(ledger.used() < held);
    let second = ledger.reserve(64).expect("second hold fits");
    let second = Evictions {
        paged: vec![event()],
        hold: second,
    };
    let (_, second_seq) = store.defer(second);
    assert_eq!(second_seq, 2);
    let held = ledger.used();
    store.acknowledge(1);
    assert_eq!(ledger.used(), held, "the unseen hold stays reserved");
    store.acknowledge(2);
    assert!(ledger.used() < held);
}

fn event() -> Paged {
    Paged {
        left: 0..1,
        right: 0..1,
        hunks: 1,
        state: PagedState::Rediffable,
    }
}

#[test]
fn an_empty_hunk_vector_is_charged_by_capacity() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let limits = limits(&ledger, false);
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let before = ledger.used();
    let parts = [part(0, Kind::Detail)];
    let _recorded = store.record(&parts, 0, (4, ledger.empty()));
    assert_eq!(store.spare_bytes, bytes_of::<Hunk>(4));
    assert!(ledger.used() >= before + bytes_of::<Hunk>(4));
}

#[test]
fn evicting_one_entry_releases_only_its_own_spare() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let mut limits = limits(&ledger, true);
    limits.reach = 100;
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let first = Part {
        pair: SectionPair {
            left: 0..1,
            right: 0..1,
        },
        end: 1,
        kind: Kind::Detail,
    };
    let second = Part {
        pair: SectionPair {
            left: 10..11,
            right: 10..11,
        },
        end: 1,
        kind: Kind::Detail,
    };
    let _first = store.record(&[first], 0, (2, ledger.empty()));
    let _second = store.record(&[second], 0, (2, ledger.empty()));
    assert_eq!(store.spare_bytes, bytes_of::<Hunk>(2));
    store.reach = 0;
    let evicted = store.page(0);
    assert_eq!(evicted.paged().len(), 1);
    assert_eq!(store.spare_bytes, bytes_of::<Hunk>(1));
}

#[test]
fn a_multi_event_deferral_waits_for_its_last_sequence() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let limits = limits(&ledger, false);
    let mut store = Store::new(
        Arc::new(context(&ledger)),
        ledger.empty(),
        &limits,
        StoreSeed {
            sections: 0,
            seq: seq(),
        },
    );
    let hold = ledger.reserve(192).expect("the hold fits");
    let evictions = Evictions {
        paged: vec![event(), event(), event()],
        hold,
    };
    let (paged, first) = store.defer(evictions);
    assert_eq!(paged.len(), 3);
    let held = ledger.used();
    store.acknowledge(first);
    assert_eq!(ledger.used(), held, "the batch hold is still open");
    store.acknowledge(first + 2);
    assert!(ledger.used() < held);
}

#[test]
fn sequences_continue_across_restarted_stores() {
    let seq = seq();
    let first_ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let first_limits = limits(&first_ledger, false);
    let mut first = Store::new(
        Arc::new(context(&first_ledger)),
        first_ledger.empty(),
        &first_limits,
        StoreSeed {
            sections: 0,
            seq: Arc::clone(&seq),
        },
    );
    let hold = first_ledger.reserve(192).expect("the hold fits");
    let events = Evictions {
        paged: vec![event(), event(), event()],
        hold,
    };
    let (_, first_seq) = first.defer(events);
    assert_eq!(first_seq, 1);
    let second_ledger = Arc::new(Ledger::new(Budget::from_mib(1)));
    let second_limits = limits(&second_ledger, false);
    let mut second = Store::new(
        Arc::new(context(&second_ledger)),
        second_ledger.empty(),
        &second_limits,
        StoreSeed { sections: 0, seq },
    );
    let hold = second_ledger.reserve(64).expect("the hold fits");
    let events = Evictions {
        paged: vec![event()],
        hold,
    };
    let (_, second_seq) = second.defer(events);
    assert_eq!(second_seq, 4);
    let held = second_ledger.used();
    second.acknowledge(first_seq);
    assert_eq!(second_ledger.used(), held, "the run-1 ack is stale");
    second.acknowledge(second_seq);
    assert!(second_ledger.used() < held);
}
