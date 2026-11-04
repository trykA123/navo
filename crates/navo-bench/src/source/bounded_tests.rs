use std::fs::{DirEntry, Metadata};
use std::time::Instant;

use super::*;
use crate::fixture;

/// Never returns from a metadata call, like a hung network or cloud folder.
struct BlockingIo {
    start: Instant,
}

impl walk::WalkIo for BlockingIo {
    fn elapsed(&mut self) -> Duration {
        self.start.elapsed()
    }

    fn metadata(&mut self, _entry: &DirEntry) -> io::Result<Metadata> {
        std::thread::sleep(Duration::from_secs(600));
        Err(io::ErrorKind::TimedOut.into())
    }
}

struct PlainIo;

impl walk::WalkIo for PlainIo {
    fn elapsed(&mut self) -> Duration {
        Duration::ZERO
    }

    fn metadata(&mut self, entry: &DirEntry) -> io::Result<Metadata> {
        entry.metadata()
    }
}

fn short() -> WalkConfig {
    WalkConfig {
        max_time: Duration::from_millis(50),
        ..WalkConfig::default()
    }
}

#[test]
fn a_blocked_walk_is_abandoned_with_a_timeout() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let io = BlockingIo {
        start: Instant::now(),
    };
    let started = Instant::now();
    let mut seen = 0;
    let summary = walk::bounded(&source, short(), (Duration::from_millis(50), io), |_| {
        seen += 1;
    });
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(seen, 0);
    assert_eq!(
        summary.outcome,
        WalkOutcome::StoppedAt {
            files: 0,
            reason: StopReason::Timeout
        }
    );
}

#[test]
fn the_bounded_walk_returns_the_workers_summary_and_records() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let mut seen = 0_u64;
    let summary = source.walk(WalkConfig::default(), |_| seen += 1);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
    assert_eq!(summary.errors, 0);
    assert_eq!(summary.files, seen);
    assert!(seen > 5);
}

#[test]
fn a_visit_that_returns_false_stops_the_walk() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let mut calls = 0;
    let visit = |_| {
        calls += 1;
        false
    };
    let summary = walk::run_with(&source, WalkConfig::default(), visit, &mut PlainIo);
    assert_eq!(calls, 1);
    assert_eq!(summary.files, 1);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
}
