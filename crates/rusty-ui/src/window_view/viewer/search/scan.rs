use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rusty_core::paging::{ByteSource, Counted, Needle, TextLayout, count};

use super::jobs::{Job, Throttle, spawn_job};
use crate::prelude::*;
use crate::window_view::viewer::files::Reopen;

pub(in crate::window_view::viewer) const KEEP: usize = 100_000;
const RUNNING_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::window_view::viewer) enum Need {
    Tail,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Scan {
    pub needle: Needle,
    pub from: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Scanned {
    pub needle: Needle,
    pub from: u64,
    pub len: u64,
    pub counted: Counted,
}

pub(in crate::window_view::viewer) enum ScanUpdate {
    Running(u64),
    Landed(Scanned),
    Failed(Needle),
    Finished,
}

pub(in crate::window_view::viewer) fn needle_width(needle: &Needle, layout: &TextLayout) -> u64 {
    match needle {
        Needle::Text { text, .. } if layout.is_utf8() => text.len() as u64,
        Needle::Text { text, .. } => layout.encode(text).map_or(0, |bytes| bytes.len() as u64),
        Needle::Bytes(bytes) => bytes.len() as u64,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Tally {
    pub needle: Needle,
    pub layout: TextLayout,
    pub counted: Counted,
    pub scanned: u64,
    pub need: Option<Need>,
}

impl Tally {
    pub(in crate::window_view::viewer) fn new(needle: Needle, layout: TextLayout) -> Self {
        Self {
            needle,
            layout,
            counted: Counted::default(),
            scanned: 0,
            need: Some(Need::Full),
        }
    }

    pub(in crate::window_view::viewer) fn width(&self) -> u64 {
        needle_width(&self.needle, &self.layout)
    }

    pub(in crate::window_view::viewer) fn source_changed(&mut self, appended_to: Option<u64>) {
        let can_append = appended_to.is_some_and(|len| len >= self.scanned);
        let need = if can_append { Need::Tail } else { Need::Full };
        self.need = self.need.max(Some(need));
    }

    fn resume_from(&self) -> u64 {
        let tail = self.scanned.saturating_sub(self.width().saturating_sub(1));
        let is_whole = self.counted.total == self.counted.starts.len() as u64;
        let last_end = self
            .counted
            .starts
            .last()
            .filter(|_| is_whole)
            .map_or(0, |start| start + self.width());
        tail.max(last_end)
    }

    pub(in crate::window_view::viewer) fn take_scan(&mut self) -> Option<Scan> {
        let from = match self.need.take()? {
            Need::Tail if self.counted.done => self.resume_from(),
            Need::Full | Need::Tail => 0,
        };
        Some(Scan {
            needle: self.needle.clone(),
            from,
        })
    }

    pub(in crate::window_view::viewer) fn land(&mut self, scanned: Scanned) {
        if scanned.from == 0 {
            self.counted = scanned.counted;
        } else {
            let room = KEEP.saturating_sub(self.counted.starts.len());
            self.counted.total += scanned.counted.total;
            self.counted
                .starts
                .extend(scanned.counted.starts.into_iter().take(room));
            self.counted.done = true;
        }
        self.scanned = scanned.len;
    }
}

struct Tail<'a> {
    source: &'a dyn ByteSource,
    from: u64,
    len: u64,
}

impl ByteSource for Tail<'_> {
    fn len(&self) -> u64 {
        self.len - self.from
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let at = self.from + offset;
        let room = usize::try_from(self.len.saturating_sub(at)).unwrap_or(usize::MAX);
        let take = buf.len().min(room);
        self.source.read_at(at, &mut buf[..take])
    }

    fn layout(&self) -> TextLayout {
        self.source.layout()
    }
}

fn scan_one(
    source: &dyn ByteSource,
    scan: Scan,
    cancel: &AtomicBool,
    send: &dyn Fn(ScanUpdate),
) -> Option<ScanUpdate> {
    let len = source.len();
    let from = source.layout().align(scan.from.min(len));
    let tail = Tail { source, from, len };
    let mut throttle = Throttle::new(RUNNING_EVERY);
    let counted = count(&tail, &scan.needle, KEEP, cancel, &mut |_, total| {
        throttle.when_due(|| send(ScanUpdate::Running(total)));
    });
    match counted {
        Ok(mut counted) if counted.done => {
            counted.starts.iter_mut().for_each(|start| *start += from);
            Some(ScanUpdate::Landed(Scanned {
                needle: scan.needle,
                from,
                len,
                counted,
            }))
        }
        Ok(_) => None,
        Err(_) => Some(ScanUpdate::Failed(scan.needle)),
    }
}

fn scan_all(source: &Reopen, scans: Vec<Scan>, cancel: &AtomicBool, send: &dyn Fn(ScanUpdate)) {
    let opened = match source() {
        Ok(opened) => opened,
        Err(_) => {
            for scan in scans {
                send(ScanUpdate::Failed(scan.needle));
            }
            return send(ScanUpdate::Finished);
        }
    };
    for scan in scans {
        match scan_one(&*opened, scan, cancel, send) {
            Some(update) => send(update),
            None => return,
        }
    }
    send(ScanUpdate::Finished);
}

pub(in crate::window_view::viewer) fn start_scans<V: 'static>(
    source: Reopen,
    scans: Vec<Scan>,
    cx: &mut Context<V>,
    apply: impl Fn(&mut V, ScanUpdate, &mut Context<V>) + 'static,
) -> Job {
    spawn_job(
        cx,
        move |cancel, send| scan_all(&source, scans, cancel, send),
        apply,
    )
}

#[cfg(test)]
mod tests {
    use rusty_core::paging::{Counted, Needle, TextLayout};

    use super::{Need, Scanned, Tally};

    fn word(text: &str) -> Needle {
        Needle::Text {
            text: text.to_owned(),
            match_case: false,
        }
    }

    fn landed(from: u64, len: u64, starts: Vec<u64>) -> Scanned {
        Scanned {
            needle: word("ab"),
            from,
            len,
            counted: Counted {
                total: starts.len() as u64,
                starts,
                done: true,
            },
        }
    }

    #[test]
    fn growth_resumes_one_needle_width_before_the_old_end() {
        let mut tally = Tally::new(word("abc"), TextLayout::default());
        let _ = tally.take_scan();
        tally.land(Scanned {
            needle: word("abc"),
            ..landed(0, 100, vec![10])
        });
        tally.source_changed(Some(150));
        assert_eq!(tally.take_scan().map(|scan| scan.from), Some(98));
    }

    #[test]
    fn appended_starts_add_to_the_total() {
        let mut tally = Tally::new(word("ab"), TextLayout::default());
        let _ = tally.take_scan();
        tally.land(landed(0, 100, vec![4, 50]));
        tally.source_changed(Some(200));
        let _ = tally.take_scan();
        tally.land(landed(99, 200, vec![120]));
        assert_eq!(tally.counted.total, 3);
        assert_eq!(tally.counted.starts, [4, 50, 120]);
        assert_eq!(tally.scanned, 200);
    }

    #[test]
    fn growth_during_a_running_scan_resumes_from_its_end_once_it_lands() {
        let mut tally = Tally::new(word("ab"), TextLayout::default());
        let _ = tally.take_scan();
        tally.source_changed(Some(150));
        assert_eq!(tally.need, Some(Need::Tail));
        tally.land(landed(0, 100, vec![4]));
        assert_eq!(tally.take_scan().map(|scan| scan.from), Some(99));
    }

    #[test]
    fn a_shrunk_or_rewritten_source_needs_a_full_scan() {
        let mut tally = Tally::new(word("ab"), TextLayout::default());
        let _ = tally.take_scan();
        tally.land(landed(0, 100, vec![4]));
        tally.source_changed(Some(50));
        assert_eq!(tally.need, Some(Need::Full));
        let _ = tally.take_scan();
        tally.land(landed(0, 50, vec![4]));
        tally.source_changed(None);
        assert_eq!(tally.take_scan().map(|scan| scan.from), Some(0));
    }

    #[test]
    fn a_failed_scan_starts_again_from_zero() {
        let mut tally = Tally::new(word("ab"), TextLayout::default());
        let _ = tally.take_scan();
        tally.source_changed(Some(150));
        assert_eq!(tally.take_scan().map(|scan| scan.from), Some(0));
    }
}
