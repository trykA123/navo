use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rusty_core::paging::{ByteSource, TextLayout};

use super::super::*;
use super::{collect, failure, request};
use crate::model::{CancelToken, Reason};

const SEEK_READ: usize = 64 * 1024;

enum Rule {
    From(u64),
    SeekStep,
}

struct Trip {
    bytes: Vec<u8>,
    token: CancelToken,
    armed: Arc<AtomicBool>,
    rule: Rule,
    seen: Mutex<HashSet<u64>>,
    late: Arc<AtomicUsize>,
}

impl Trip {
    fn trips(&self, offset: u64, len: usize) -> bool {
        match self.rule {
            Rule::From(at) => offset >= at,
            Rule::SeekStep if len == SEEK_READ => {
                let mut seen = self.seen.lock().unwrap();
                let step =
                    offset >= SEEK_READ as u64 && seen.contains(&(offset - SEEK_READ as u64));
                seen.insert(offset);
                step
            }
            Rule::SeekStep => false,
        }
    }
}

impl ByteSource for Trip {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.token.is_cancelled() {
            self.late.fetch_add(1, Ordering::SeqCst);
        } else if self.armed.load(Ordering::SeqCst) && self.trips(offset, buf.len()) {
            self.token.cancel();
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

fn long_lines(lines: usize, width: usize) -> (String, String) {
    let pad = "-".repeat(width - 9);
    let mut left = String::new();
    let mut right = String::new();
    for line in 0..lines {
        left.push_str(&format!("{line:08x}{pad}\n"));
        let mark = if line % 97 == 50 { "m" } else { "" };
        right.push_str(&format!("{line:08x}{mark}{pad}\n"));
    }
    (left, right)
}

struct Setup {
    width: usize,
    rule: fn(u64) -> Rule,
    batch_lines: u64,
    focus: u64,
}

fn cancelled_reads(setup: &Setup) -> usize {
    let (left, right) = long_lines(6_000, setup.width);
    let token = CancelToken::new();
    let armed = Arc::new(AtomicBool::new(false));
    let late = Arc::new(AtomicUsize::new(0));
    let trip = |text: &str, rule: Rule| LargeSide {
        source: Arc::new(Trip {
            bytes: text.as_bytes().to_vec(),
            token: token.clone(),
            armed: Arc::clone(&armed),
            rule,
            seen: Mutex::new(HashSet::new()),
            late: Arc::clone(&late),
        }),
        layout: TextLayout::default(),
    };
    let arm = Arc::clone(&armed);
    let forge: Forge = Arc::new(
        move |_: &mut Vec<scan::Candidate>, _: &mut Vec<scan::Candidate>| {
            arm.store(true, Ordering::SeqCst);
        },
    );
    let left = trip(&left, (setup.rule)(left.len() as u64));
    let right = trip(&right, Rule::From(u64::MAX));
    let diff = LargeDiff::start_tuned(
        request(left, right, setup.focus),
        &token,
        Tuning {
            batch_lines: setup.batch_lines,
            forge: Some(forge),
        },
    );
    let events = collect(&diff);
    assert_eq!(failure(&events), Reason::Stopped);
    late.load(Ordering::SeqCst)
}

#[test]
fn cancel_in_a_later_batch_stops_each_reader_within_one_read() {
    let late = cancelled_reads(&Setup {
        width: 1_024,
        rule: |len| Rule::From(len / 2),
        batch_lines: 1,
        focus: 0,
    });
    assert!(late <= WORKERS, "{late} reads started after cancel");
}

#[test]
fn cancel_in_a_sparse_seek_stops_each_reader_within_one_read() {
    let late = cancelled_reads(&Setup {
        width: 4_096,
        rule: |_| Rule::SeekStep,
        batch_lines: 64,
        focus: 3_000,
    });
    assert!(late <= WORKERS, "{late} reads started after cancel");
}
