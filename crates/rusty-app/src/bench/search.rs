use std::cmp::Reverse;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock, TryLockError};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_index::host::Host;
use rusty_index::pipe::{self, Reply};
use rusty_index::table::NameTable;
use rusty_ui::command_center::{
    Cancel, HitSink, IndexHooks, IndexMode, IndexOptions, IndexSource, Query, Request, ResultSource,
};

use super::report::{self, Search, percentile};

pub(super) const WARM_RUNS: usize = 50;
const LIMIT: usize = 500;
const QUIET_FOR: Duration = Duration::from_secs(2);
const QUIET_LIMIT: Duration = Duration::from_secs(120);
const QUIET_POLL: Duration = Duration::from_millis(50);
const QUERIES: [(&str, &str); 11] = [
    ("exact name", "brake_controller_main.c"),
    ("prefix", "brake_contr"),
    ("fuzzy, dropped letters", "brkcntrllrmain"),
    ("fuzzy, swapped letters", "barke_controller"),
    ("extension filter", "ext:a2l calib"),
    ("extension only", "ext:map"),
    ("path fragment", "huge-100k/sensor"),
    ("unicode, Latin", "größe"),
    ("unicode, CJK", "日本語"),
    ("one letter", "a"),
    ("no match", "zzqxj_nomatch_9472"),
];

pub(super) fn run_when_quiet(root: &Location, host: &Host) -> (f64, Vec<Search>) {
    let waited = wait_until_quiet(host);
    (waited, run(root, host.table()))
}

fn wait_until_quiet(host: &Host) -> f64 {
    let started = Instant::now();
    let table = host.table();
    let count = || table.read().unwrap_or_else(PoisonError::into_inner).len();
    let (mut last, mut since) = (count(), Instant::now());
    while started.elapsed() < QUIET_LIMIT {
        std::thread::sleep(QUIET_POLL);
        let scanning = host
            .try_status()
            .is_some_and(|status| status.progress.is_some());
        let now = count();
        if scanning || now != last {
            (last, since) = (now, Instant::now());
        } else if since.elapsed() >= QUIET_FOR {
            break;
        }
    }
    report::ms(started.elapsed())
}

fn source(root: &Location, table: Arc<RwLock<NameTable>>, waited: Arc<AtomicUsize>) -> IndexSource {
    let root = root.clone();
    let hooks = IndexHooks {
        host: Arc::new(move |_, query, cancel, _| {
            let guard = match table.try_read() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => {
                    waited.fetch_add(1, Ordering::Relaxed);
                    table.read().unwrap_or_else(PoisonError::into_inner)
                }
            };
            let mut hits = crate::search_hooks::search_table(&root, &guard, query, cancel);
            drop(guard);
            hits.sort_by_key(|hit| Reverse(hit.score));
            hits.truncate(pipe::MAX_HITS);
            Reply {
                hits,
                ..Reply::default()
            }
        }),
        ..IndexHooks::none()
    };
    let options = IndexOptions {
        mode: IndexMode::Walk,
        ..IndexOptions::default()
    };
    IndexSource::new(options, hooks, Arc::new(|_| Vec::new()))
}

pub(super) fn run(root: &Location, table: Arc<RwLock<NameTable>>) -> Vec<Search> {
    let waited = Arc::new(AtomicUsize::new(0));
    let source = source(root, table, waited.clone());
    let mut generation = 0;
    let mut once = |text: &str| {
        generation += 1;
        let request = Request {
            query: Query::parse(text),
            generation,
            here: None,
            limit: LIMIT,
        };
        let sink = HitSink::new(generation);
        let started = Instant::now();
        source.search(&request, &sink, &Cancel::new());
        let elapsed = report::ms(started.elapsed());
        (elapsed, sink.drain(generation).len())
    };
    QUERIES
        .iter()
        .map(|&(label, text)| {
            let before = waited.load(Ordering::Relaxed);
            let (cold_ms, results) = once(text);
            let mut warm: Vec<f64> = (0..WARM_RUNS).map(|_| once(text).0).collect();
            warm.sort_by(f64::total_cmp);
            let lock_waits = waited.load(Ordering::Relaxed) - before;
            Search {
                label: label.to_owned(),
                query: text.to_owned(),
                results,
                cold_ms,
                warm_runs: WARM_RUNS,
                lock_waits,
                p50_ms: percentile(&warm, 50),
                p95_ms: percentile(&warm, 95),
                p99_ms: percentile(&warm, 99),
                max_ms: warm.last().copied().unwrap_or_default(),
            }
        })
        .collect()
}
