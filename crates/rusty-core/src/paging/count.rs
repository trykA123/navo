use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use super::search::{Matcher, fill};
use super::{ByteSource, Needle, SEARCH_CHUNK};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counted {
    pub total: u64,
    pub starts: Vec<u64>,
    pub done: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "signature fixed by the P15 A5 contract"
)]
pub fn count<S: ByteSource + ?Sized>(
    src: &S,
    needle: &Needle,
    keep: usize,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Counted, String> {
    let Some((matcher, width)) = Matcher::new(needle, &src.layout())? else {
        return Ok(Counted {
            done: true,
            ..Counted::default()
        });
    };
    let mut tally = Tally {
        counted: Counted::default(),
        keep,
        free_from: 0,
        width: width as u64,
    };
    let len = src.len();
    let mut buf = Vec::new();
    let mut at = 0;
    while at < len {
        if cancel.load(Ordering::Relaxed) {
            return Ok(tally.counted);
        }
        let end = (at + SEARCH_CHUNK as u64).min(len);
        let from = tally.free_from.max(at);
        if from < end {
            let hay = fill(src, from, (end + tally.width - 1).min(len), &mut buf)?;
            tally.add(matcher.starts(hay), from..end);
        }
        at = end;
        progress(at, tally.counted.total);
    }
    tally.counted.done = true;
    Ok(tally.counted)
}

struct Tally {
    counted: Counted,
    keep: usize,
    free_from: u64,
    width: u64,
}

impl Tally {
    fn add(&mut self, found: Vec<usize>, window: Range<u64>) {
        let starts = found.into_iter().map(|start| window.start + start as u64);
        for start in starts.take_while(|start| *start < window.end) {
            self.counted.total += 1;
            if self.counted.starts.len() < self.keep {
                self.counted.starts.push(start);
            }
            self.free_from = start + self.width;
        }
    }
}

#[must_use]
pub fn ordinal(counted: &Counted, at: u64) -> Option<u64> {
    counted
        .starts
        .binary_search(&at)
        .ok()
        .map(|index| index as u64 + 1)
}
