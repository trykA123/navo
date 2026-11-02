use std::time::Duration;

use super::*;

fn hunk_event() -> DiffEvent {
    DiffEvent::Hunks(Covered {
        left: 10..11,
        right: 10..11,
        hunks: vec![Hunk {
            left_lines: 10..11,
            right_lines: 10..11,
            left_bytes: 0..2,
            right_bytes: 0..2,
        }],
        coarse: Vec::new(),
        paged: Vec::new(),
    })
}

fn two_section_store(ledger: &Arc<Ledger>) -> Store {
    let mut limits = limits(ledger, true);
    limits.reach = 100;
    let seed = StoreSeed {
        sections: 2,
        seq: seq(),
    };
    let mut store = Store::new(Arc::new(context(ledger)), ledger.empty(), &limits, seed);
    let mut parts = vec![part(4, Kind::Detail), part(8, Kind::Detail)];
    parts[1].pair = SectionPair {
        left: 10..11,
        right: 10..11,
    };
    let _recorded = store.record(&parts, 0, (8, ledger.empty()));
    store.reach = 0;
    store
}

/// A finished run: two detailed sections, a full event channel ending in a `Hunks` event,
/// and a `before_done` that moves the focus onto the second section.
fn finishing_run(ledger: &Arc<Ledger>) -> (LargeDiff, Driver) {
    let store = two_section_store(ledger);
    let (events, receive) = std::sync::mpsc::sync_channel(EVENT_BUFFER);
    let progress = Progress {
        stage: Stage::Sections,
        done: 0,
        total: 1,
    };
    for _ in 1..EVENT_BUFFER {
        events.try_send(DiffEvent::Progress(progress)).unwrap();
    }
    events.try_send(hunk_event()).unwrap();
    let diff = LargeDiff {
        events: receive,
        focus: Arc::new(AtomicU64::new(0)),
        focus_gate: Arc::default(),
        cancel: CancelToken::new(),
        outcome: Arc::default(),
        store: Arc::new(std::sync::Mutex::new(Some(store))),
        ledger: Arc::clone(ledger),
        done: Arc::new(AtomicBool::new(false)),
    };
    let tuning = Tuning {
        before_done: Some(Arc::new(|focus| focus.store(10, Ordering::Relaxed))),
        ..Tuning::default()
    };
    let driver = Driver {
        left: side(b"a\n"),
        right: side(b"b\n"),
        options: Default::default(),
        cancel: diff.cancel.clone(),
        events,
        focus: Arc::clone(&diff.focus),
        focus_gate: Arc::clone(&diff.focus_gate),
        seq: seq(),
        outcome: Arc::clone(&diff.outcome),
        tuning,
        store: Arc::clone(&diff.store),
        ledger: Arc::clone(ledger),
        extras: Arc::default(),
        lingering: std::sync::Mutex::default(),
        done: Arc::clone(&diff.done),
    };
    (diff, driver)
}

fn wait_until_first_section_is_paged(diff: &LargeDiff) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while diff.store.lock().unwrap().as_ref().unwrap().entries[0].detailed {
        assert!(
            std::time::Instant::now() < deadline,
            "finish reaches paging"
        );
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

#[test]
fn focus_evictions_wait_until_done_is_sent() {
    let ledger = Arc::new(Ledger::new(Budget::from_mib(16)));
    let (diff, driver) = finishing_run(&ledger);
    let summary = Summary {
        left_lines: 11,
        right_lines: 11,
        sections: 2,
        dropped_anchors: 0,
        restarts: 0,
        extra_workers: 0,
    };
    let finish = std::thread::spawn(move || {
        driver.before_done();
        driver.finish(Ok(summary));
    });
    wait_until_first_section_is_paged(&diff);
    let evictions = diff.set_focus(100);
    let premature = !evictions.paged().is_empty();
    evictions.acknowledge();
    let received = collect(&diff);
    finish.join().unwrap();
    assert!(!premature, "queued Hunks must precede direct evictions");
    assert!(matches!(received.last(), Some(DiffEvent::Done(Ok(_)))));
    let at = |wanted: fn(&DiffEvent) -> bool| received.iter().position(wanted).unwrap();
    let hunks_at = at(|event| matches!(event, DiffEvent::Hunks(_)));
    let paged_at =
        at(|event| matches!(event, DiffEvent::Paged { paged, .. } if paged.left == (10..11)));
    assert!(hunks_at < paged_at, "the late eviction follows its hunks");
    let evictions = diff.set_focus(100);
    assert!(
        evictions.paged().is_empty(),
        "the driver applied the late focus"
    );
    evictions.acknowledge();
}
