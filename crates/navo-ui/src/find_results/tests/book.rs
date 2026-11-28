//! Bookkeeping tests for the find results window: tabs, drain cadence, budget and persistence.

use std::ops::ControlFlow;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, Location, ProviderPath, SavedFind};
use navo_fs::find::{FindError, FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::command_center::content_source::ContentWalk;
use crate::find_results::book::{
    FindBook, MAX_EVENTS_PER_DRAIN, MAX_TABS, MEMORY_BUDGET, Notice, ROW_REBUILD_INTERVAL,
    TRIM_LINES, UnreachableRoot,
};
use crate::find_results::rows::{LinesShown, Row, SortKey};
use crate::find_results::run::{FindRun, RunEvent};
use crate::find_results::store::{HitStore, StoreStop};

const WAIT: Duration = Duration::from_secs(10);

fn location(text: &str) -> Location {
    Location::new(text).expect("a valid test location")
}

fn found(relative: &str, lines: u64, total: u32) -> FoundFile {
    FoundFile {
        relative: ProviderPath::from_text(relative),
        entry: FsEntry {
            name: ProviderPath::from_text(relative),
            kind: EntryKind::File,
            size: 16,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        },
        hits: FileHits {
            hits: (1..=lines)
                .map(|line| LineHit {
                    line,
                    column: 1,
                    text: "a matched hit line with enough bytes for memory tests".into(),
                })
                .collect(),
            total,
            skipped: None,
        },
    }
}

fn batch_with(files: Vec<FoundFile>, scanned: u64) -> FoundBatch {
    FoundBatch {
        files,
        progress: FindProgress {
            scanned,
            ..FindProgress::default()
        },
    }
}

fn fill_store(store: &mut HitStore, folders: usize, lines_per_file: u64) {
    for folder in 0..folders {
        let rel = format!("dir{folder:02}/file.rs");
        let total = u32::try_from(lines_per_file).unwrap_or(u32::MAX);
        store
            .push(0, &batch_with(vec![found(&rel, lines_per_file, total)], 1))
            .expect("synthetic batch fits");
    }
}

fn attach_channel(book: &mut FindBook, id: u64, capacity: usize) -> mpsc::SyncSender<RunEvent> {
    let (send, receive) = mpsc::sync_channel(capacity);
    let tab = book.tab_mut(id).expect("tab exists");
    tab.run = Some(FindRun::from_receiver(receive));
    send
}

#[test]
fn constants_match_the_contract() {
    assert_eq!(MAX_TABS, 12);
    assert_eq!(MEMORY_BUDGET, 48 * 1024 * 1024);
    assert_eq!(TRIM_LINES, 3);
    assert_eq!(MAX_EVENTS_PER_DRAIN, 16);
    assert_eq!(ROW_REBUILD_INTERVAL, Duration::from_millis(100));
}

#[test]
fn tab_navigation_and_closing_keep_active_selection_consistent() {
    let mut book = FindBook::new();
    assert!(book.is_empty());
    assert!(book.active().is_none());
    book.next();
    book.previous();
    assert!(!book.close(99));

    let a = book.add("alpha", location("/tmp/a"));
    let b = book.add("beta", location("/tmp/b"));
    let c = book.add("gamma", location("/tmp/c"));
    assert_eq!(book.len(), 3);
    assert_eq!(book.active().map(|tab| tab.id), Some(c));

    book.next();
    assert_eq!(book.active().map(|tab| tab.id), Some(a));
    book.previous();
    assert_eq!(book.active().map(|tab| tab.id), Some(c));
    book.previous();
    assert_eq!(book.active().map(|tab| tab.id), Some(b));

    // Closing a tab before active keeps active pointing at the same tab (`b`).
    assert!(book.close(a));
    assert_eq!(book.active().map(|tab| tab.id), Some(b));
    assert_eq!(book.active_index(), 0);

    // Closing the active last tab falls back to the remaining tab.
    assert!(book.select(c));
    assert!(book.close(c));
    assert_eq!(book.active().map(|tab| tab.id), Some(b));

    assert!(book.close(b));
    assert!(book.is_empty());
    assert!(book.active().is_none());
}

#[test]
fn thirteenth_tab_evicts_oldest_finished_tab_before_running_tabs() {
    let mut book = FindBook::new();
    let mut ids = Vec::new();
    let mut senders = Vec::new();
    for i in 0..MAX_TABS {
        ids.push(book.add(format!("q{i}"), location("/tmp/base")));
    }
    // Keep tabs 0 and 1 running; tab 2 is the oldest finished tab.
    senders.push(attach_channel(&mut book, ids[0], 4));
    senders.push(attach_channel(&mut book, ids[1], 4));

    let thirteenth = book.add("q12", location("/tmp/base"));
    assert_eq!(book.len(), MAX_TABS);
    assert!(book.tab(ids[0]).is_some(), "running tab 0 stays");
    assert!(book.tab(ids[1]).is_some(), "running tab 1 stays");
    assert!(book.tab(ids[2]).is_none(), "oldest finished tab 2 dropped");
    assert_eq!(book.active().map(|tab| tab.id), Some(thirteenth));
    drop(senders);
}

#[test]
fn thirteenth_tab_drops_oldest_and_cancels_when_all_twelve_are_running() {
    let mut book = FindBook::new();
    let (cancel_send, cancel_rx) = mpsc::sync_channel(1);
    let first_id = book.add("q0", location("/tmp/base"));
    let blocking_walk: ContentWalk = Arc::new(move |_, _, cancel, sink| {
        while sink(batch_with(vec![found("a.rs", 1, 1)], 1)) == ControlFlow::Continue(()) {}
        let _ = cancel_send.send(cancel.is_cancelled());
        Err(FindError::Cancelled)
    });
    book.tab_mut(first_id).expect("first").start(blocking_walk);
    // Wait until the background walk is actually running before evicting its tab.
    let first_event = book
        .tab(first_id)
        .expect("first")
        .run
        .as_ref()
        .expect("run active")
        .receive
        .recv_timeout(WAIT);
    assert!(matches!(first_event, Ok(RunEvent::Batch { root: 0, .. })));

    let mut senders = Vec::new();
    for i in 1..MAX_TABS {
        let id = book.add(format!("q{i}"), location("/tmp/base"));
        senders.push(attach_channel(&mut book, id, 4));
    }

    let thirteenth = book.add("q12", location("/tmp/base"));
    assert_eq!(book.len(), MAX_TABS);
    assert!(book.tab(first_id).is_none(), "oldest running tab dropped");
    assert_eq!(book.active().map(|tab| tab.id), Some(thirteenth));
    assert_eq!(cancel_rx.recv_timeout(WAIT), Ok(true));
    drop(senders);
}

#[test]
fn enforce_budget_trims_oldest_finished_tab_first_and_stops_when_under_budget() {
    // Measure one full tab (20 folders x 50 lines) vs trimmed (3 lines).
    let mut probe = HitStore::default();
    fill_store(&mut probe, 20, 50);
    let heavy_bytes = probe.memory_bytes();
    probe.trim_lines(TRIM_LINES).expect("trim fits");
    let trimmed_bytes = probe.memory_bytes();
    assert!(trimmed_bytes < heavy_bytes);

    // Budget fits one heavy tab plus one trimmed tab, but NOT two heavy tabs.
    let budget = heavy_bytes + trimmed_bytes + 4_096;
    assert!(budget < heavy_bytes * 2);

    let mut book = FindBook::with_budget(budget);
    // Tab 0 already has <= 3 lines/file: must not be falsely flagged Trimmed.
    let small = book.add("small", location("/tmp/s"));
    fill_store(&mut book.tab_mut(small).expect("small").store, 1, 2);

    let oldest = book.add("oldest", location("/tmp/o"));
    fill_store(&mut book.tab_mut(oldest).expect("oldest").store, 20, 50);

    let newer = book.add("newer", location("/tmp/n"));
    fill_store(&mut book.tab_mut(newer).expect("newer").store, 20, 50);

    assert!(book.memory_bytes() > budget);
    book.enforce_budget();

    assert!(book.memory_bytes() <= budget);
    assert_eq!(book.tab(small).expect("small").notice, None);
    assert_eq!(
        book.tab(oldest).expect("oldest").notice,
        Some(Notice::Trimmed)
    );
    assert_eq!(
        book.tab(oldest)
            .expect("oldest")
            .store
            .file(0)
            .expect("file")
            .lines()
            .count(),
        TRIM_LINES
    );
    assert_eq!(book.tab(newer).expect("newer").notice, None);
    assert_eq!(
        book.tab(newer)
            .expect("newer")
            .store
            .file(0)
            .expect("file")
            .lines()
            .count(),
        50
    );
}

#[test]
fn enforce_budget_stops_lines_on_running_tab_alone_over_budget() {
    let mut probe = HitStore::default();
    fill_store(&mut probe, 10, 40);
    let budget = probe.memory_bytes() / 2;

    let mut book = FindBook::with_budget(budget);
    let id = book.add("heavy", location("/tmp/h"));
    let send = attach_channel(&mut book, id, 8);
    let now = Instant::now();

    // First batch pushes the sole running tab past the budget.
    let files = (0..10)
        .map(|i| found(&format!("d{i}/f.rs"), 40, 40))
        .collect();
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(files, 10),
    })
    .expect("send batch 1");
    // Second batch arrives after stop_lines kicked in: hits count, zero lines stored.
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("extra/g.rs", 5, 5)], 11),
    })
    .expect("send batch 2");

    assert!(book.drain(now));
    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.notice, Some(Notice::LinesStopped));
    assert_eq!(tab.store.stopped(), Some(StoreStop::Lines));
    let extra = tab.store.file(10).expect("extra file stored");
    assert_eq!(extra.hits, 5);
    assert_eq!(extra.lines().count(), 0);
}

#[test]
fn drain_caps_at_sixteen_events_per_call_and_respects_hundred_ms_row_cadence() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/p"));
    let send = attach_channel(&mut book, id, 32);
    for i in 0..25 {
        send.send(RunEvent::Batch {
            root: 0,
            batch: batch_with(vec![found(&format!("f{i:02}.rs"), 1, 1)], (i + 1) as u64),
        })
        .expect("queue batch");
    }

    let t0 = Instant::now();
    // First drain processes at most 16 events and builds rows immediately (first build).
    assert!(book.drain(t0));
    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.store.file_count(), 16);
    assert_eq!(tab.rows.len(), 1 + 16 * 2);

    // At t0 + 50 ms, remaining 9 events drain into store, but rows wait for the 100 ms mark.
    let t50 = t0 + Duration::from_millis(50);
    assert!(book.drain(t50));
    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.store.file_count(), 25);
    assert_eq!(
        tab.rows.len(),
        1 + 16 * 2,
        "rows do not rebuild before 100 ms"
    );

    // At t0 + 99 ms with no events, still too early to rebuild.
    assert!(!book.drain(t0 + Duration::from_millis(99)));
    assert_eq!(book.tab(id).expect("tab").rows.len(), 1 + 16 * 2);

    // At t0 + 100 ms with no new events, pending store changes flush into rows.
    let t100 = t0 + ROW_REBUILD_INTERVAL;
    assert!(book.drain(t100));
    assert_eq!(book.tab(id).expect("tab").rows.len(), 1 + 25 * 2);

    // Idle call with clean store reports false.
    assert!(!book.drain(t100 + Duration::from_millis(200)));

    // RunEvent::Done finishes the folder index and rebuilds immediately even before 100 ms.
    let before_done_bytes = book.memory_bytes();
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("newdir/last.rs", 1, 1)], 26),
    })
    .expect("send last batch");
    send.send(RunEvent::Done(Ok(FindProgress {
        scanned: 26,
        hits: 26,
        matched: 26,
        ..FindProgress::default()
    })))
    .expect("send done");

    assert!(book.drain(t100 + Duration::from_millis(10)));
    let tab = book.tab(id).expect("tab");
    assert!(!tab.is_running());
    assert_eq!(tab.progress.scanned, 26);
    assert_eq!(tab.store.file_count(), 26);
    assert!(tab.rows.contains(&Row::File(25)));
    assert!(
        tab.store.memory_bytes() < before_done_bytes + 512,
        "finish_index freed the folder index"
    );
}

#[test]
fn done_calls_finish_index_and_frees_folder_index_memory() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/idx"));
    let send = attach_channel(&mut book, id, 4);
    let files: Vec<FoundFile> = (0..60)
        .map(|i| found(&format!("deep/folder/name_{i:03}/a.rs"), 1, 1))
        .collect();
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(files, 60),
    })
    .expect("batch");
    let t0 = Instant::now();
    assert!(book.drain(t0));
    let running_bytes = book.memory_bytes();

    send.send(RunEvent::Done(Ok(FindProgress {
        scanned: 60,
        ..FindProgress::default()
    })))
    .expect("done");
    assert!(book.drain(t0 + Duration::from_millis(10)));
    let finished_bytes = book.memory_bytes();
    assert!(
        finished_bytes < running_bytes,
        "finish_index should drop memory from {running_bytes} to {finished_bytes}"
    );
}

#[test]
fn drain_preserves_cursor_by_row_identity_and_falls_back_to_file() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/c"));
    let send = attach_channel(&mut book, id, 8);
    let t0 = Instant::now();

    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(
            vec![found("a/first.rs", 2, 2), found("b/second.rs", 2, 2)],
            2,
        ),
    })
    .expect("batch 1");
    assert!(book.drain(t0));

    // Park the cursor on Row::File(1) ("b/second.rs") and switch to Name sort.
    {
        let tab = book.tab_mut(id).expect("tab");
        tab.cursor = Some(Row::File(1));
        tab.mode.sort = Some((SortKey::Name, false));
    }
    let before_index = book.tab(id).expect("tab").cursor_index();

    // New file arrives in folder `a` (inserted above folder `b`), shifting Row::File(1)'s index.
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("a/middle.rs", 1, 1)], 3),
    })
    .expect("batch 2");
    assert!(book.drain(t0 + ROW_REBUILD_INTERVAL));
    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.cursor, Some(Row::File(1)));
    assert!(tab.cursor_index() > before_index);

    // Park cursor on a Line row, then hide lines: nearest falls back to Row::File(1).
    {
        let tab = book.tab_mut(id).expect("tab");
        tab.cursor = Some(Row::Line { file: 1, nth: 1 });
        tab.mode.lines = LinesShown::Off;
        tab.rebuild_rows(t0 + Duration::from_millis(200));
    }
    assert_eq!(book.tab(id).expect("tab").cursor, Some(Row::File(1)));
}

#[test]
fn trim_all_compacts_every_tab_and_rebuilds_rows_when_lines_disappear() {
    let mut book = FindBook::new();
    let a = book.add("a", location("/tmp/a"));
    let b = book.add("b", location("/tmp/b"));
    fill_store(&mut book.tab_mut(a).expect("a").store, 10, 40);
    fill_store(&mut book.tab_mut(b).expect("b").store, 10, 40);
    let now = Instant::now();
    book.tab_mut(a).expect("a").rebuild_rows(now);
    book.tab_mut(a).expect("a").cursor = Some(Row::Line { file: 0, nth: 2 });

    let before = book.memory_bytes();
    book.trim_all(TRIM_LINES);
    let after_three = book.memory_bytes();
    assert!(after_three < before);
    // Closing-window trim does not attach a budget-overflow notice.
    assert_eq!(book.tab(a).expect("a").notice, None);

    // Trimming below 3 lines removes vanished Line rows and moves cursor to its File row.
    book.trim_all(1);
    let tab_a = book.tab(a).expect("a");
    assert!(!tab_a.rows.contains(&Row::Line { file: 0, nth: 2 }));
    assert_eq!(tab_a.cursor, Some(Row::File(0)));
}

#[test]
fn saved_and_restore_round_trip_without_starting_runs() {
    let mut book = FindBook::new();
    book.add("   ", location("/tmp/blank"));
    book.add("needle ext:rs", location("/tmp/one"));
    book.add("alpha", location("/tmp/two"));

    let saved = book.saved();
    assert_eq!(
        saved,
        vec![
            SavedFind {
                text: "needle ext:rs".into(),
                base: location("/tmp/one"),
            },
            SavedFind {
                text: "alpha".into(),
                base: location("/tmp/two"),
            },
        ]
    );

    let mut restored = FindBook::new();
    restored.add("stale", location("/tmp/stale"));
    restored.restore(saved.clone());
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.saved(), saved);
    for tab in restored.tabs() {
        assert!(!tab.is_running(), "restored tabs must not start running");
        assert!(tab.rows.is_empty());
    }
    assert_eq!(restored.tabs()[0].query.text, "needle");
    assert_eq!(restored.tabs()[0].query.ext, vec!["rs"]);
}

#[test]
fn drain_records_failed_and_unresolvable_roots_cleanly_per_fr_d12() {
    let mut book = FindBook::new();
    let id = book.add(
        "needle in:good in:broken in:../../../../escape",
        location("/tmp/navo-book/base"),
    );
    let walk: ContentWalk = Arc::new(|root, _, _, sink| {
        if root.as_str().ends_with("broken") {
            return Err(FindError::Invalid("permission denied".into()));
        }
        let _ = sink(batch_with(vec![found("ok.rs", 1, 1)], 4));
        Ok(FindProgress {
            scanned: 4,
            hits: 1,
            matched: 1,
            ..FindProgress::default()
        })
    });
    book.tab_mut(id).expect("tab").start(walk);

    let deadline = Instant::now() + WAIT;
    let mut clock = Instant::now();
    while book.tab(id).expect("tab").is_running() {
        assert!(Instant::now() < deadline, "run did not finish in time");
        clock += Duration::from_millis(10);
        book.drain(clock);
        std::thread::yield_now();
    }

    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.error, None);
    assert_eq!(tab.store.file_count(), 1);
    assert_eq!(tab.progress.scanned, 4);
    assert_eq!(tab.unreachable.len(), 2);

    assert!(matches!(
        &tab.unreachable[0],
        UnreachableRoot::Unresolved { within, error }
            if within == "../../../../escape" && !error.is_empty()
    ));
    assert!(
        tab.unreachable[0]
            .to_string()
            .starts_with("../../../../escape: "),
        "display prefixes unresolved error with within text: {}",
        tab.unreachable[0]
    );
    assert!(matches!(
        &tab.unreachable[1],
        UnreachableRoot::WalkFailed { location: loc, error }
            if loc.as_str().ends_with("/broken") && error.contains("permission denied")
    ));
}

#[test]
fn disconnected_worker_finishes_run_and_records_error() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/disc"));
    let send = attach_channel(&mut book, id, 4);
    drop(send);

    assert!(book.drain(Instant::now()));
    let tab = book.tab(id).expect("tab");
    assert!(!tab.is_running());
    assert!(tab.error.is_some());
}

#[test]
fn drain_shares_the_sixteen_event_cap_fairly_across_running_tabs() {
    let mut book = FindBook::new();
    let a = book.add("alpha", location("/tmp/a"));
    let b = book.add("beta", location("/tmp/b"));
    let send_a = attach_channel(&mut book, a, 16);
    let send_b = attach_channel(&mut book, b, 16);
    for i in 0..12 {
        send_a
            .send(RunEvent::Batch {
                root: 0,
                batch: batch_with(vec![found(&format!("a{i}.rs"), 1, 1)], (i + 1) as u64),
            })
            .expect("queue a");
        send_b
            .send(RunEvent::Batch {
                root: 0,
                batch: batch_with(vec![found(&format!("b{i}.rs"), 1, 1)], (i + 1) as u64),
            })
            .expect("queue b");
    }

    let t0 = Instant::now();
    assert!(book.drain(t0));
    assert_eq!(book.tab(a).expect("a").store.file_count(), 8);
    assert_eq!(book.tab(b).expect("b").store.file_count(), 8);

    assert!(book.drain(t0 + ROW_REBUILD_INTERVAL));
    assert_eq!(book.tab(a).expect("a").store.file_count(), 12);
    assert_eq!(book.tab(b).expect("b").store.file_count(), 12);
}

#[test]
fn enforce_budget_trims_finished_tabs_without_stopping_a_running_tab() {
    let mut probe = HitStore::default();
    fill_store(&mut probe, 20, 50);
    let heavy_bytes = probe.memory_bytes();
    probe.trim_lines(TRIM_LINES).expect("trim fits");
    let trimmed_bytes = probe.memory_bytes();

    let budget = heavy_bytes + trimmed_bytes + 4_096;
    assert!(budget < heavy_bytes * 2);

    let mut book = FindBook::with_budget(budget);
    let running = book.add("running", location("/tmp/r"));
    let _send = attach_channel(&mut book, running, 4);
    fill_store(&mut book.tab_mut(running).expect("running").store, 20, 50);

    let finished = book.add("finished", location("/tmp/f"));
    fill_store(&mut book.tab_mut(finished).expect("finished").store, 20, 50);

    book.enforce_budget();
    assert!(book.memory_bytes() <= budget);
    assert_eq!(book.tab(running).expect("running").notice, None);
    assert_eq!(book.tab(running).expect("running").store.stopped(), None);
    assert_eq!(
        book.tab(finished).expect("finished").notice,
        Some(Notice::Trimmed)
    );
}

#[test]
fn offset_overflow_during_drain_sets_notice() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/ovf"));
    book.tab_mut(id).expect("tab").store = HitStore::with_limit(8);
    let send = attach_channel(&mut book, id, 4);
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("a_very_long_filename.rs", 1, 1)], 1),
    })
    .expect("send");

    assert!(book.drain(Instant::now()));
    let tab = book.tab(id).expect("tab");
    assert_eq!(tab.notice, Some(Notice::OffsetsFull));
    assert_eq!(tab.store.file_count(), 0);
}

#[test]
fn multi_root_progress_accumulates_without_jumping_backward() {
    let mut book = FindBook::new();
    let id = book.add("needle", location("/tmp/multi"));
    let send = attach_channel(&mut book, id, 8);
    let t0 = Instant::now();

    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("a.rs", 1, 1)], 10),
    })
    .expect("root 0 batch 1");
    send.send(RunEvent::Batch {
        root: 0,
        batch: batch_with(vec![found("b.rs", 1, 1)], 25),
    })
    .expect("root 0 batch 2");
    send.send(RunEvent::Batch {
        root: 1,
        batch: batch_with(vec![found("c.rs", 1, 1)], 4),
    })
    .expect("root 1 batch 1");

    assert!(book.drain(t0));
    assert_eq!(book.tab(id).expect("tab").progress.scanned, 29);
}
