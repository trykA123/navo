use std::ops::Range;

use rusty_core::marks::{MarkError, MarkSet};
use rusty_core::paging::{Counted, Needle, TextLayout};

use super::Marked;
use super::count::ranges_in;
use super::jobs::Job;
use super::scan::{ScanUpdate, Tally, start_scans};
use crate::prelude::*;

#[derive(Default)]
pub(in crate::window_view::viewer) struct MarkLayer {
    pub set: MarkSet,
    pub rev: u64,
    pub layout: TextLayout,
    hits: Vec<Tally>,
    job: Option<Job>,
}

impl MarkLayer {
    fn colour_of(&self, hits: &Tally) -> Option<u8> {
        self.set.colour_of(&hits.needle)
    }

    pub(in crate::window_view::viewer) fn ranges_in(
        &self,
        span: Range<u64>,
    ) -> Vec<(Range<u64>, u8)> {
        let mut ranges = Vec::new();
        for hits in &self.hits {
            let Some(colour) = self.colour_of(hits) else {
                continue;
            };
            let found = ranges_in(&hits.counted.starts, hits.width(), span.clone());
            ranges.extend(found.into_iter().map(|range| (range, colour)));
        }
        ranges
    }

    pub(in crate::window_view::viewer) fn spans(&self) -> Vec<(u64, u8)> {
        self.hits
            .iter()
            .filter_map(|hits| Some((hits, self.colour_of(hits)?)))
            .flat_map(|(hits, colour)| {
                hits.counted
                    .starts
                    .iter()
                    .map(move |start| (*start, colour))
            })
            .collect()
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn is_scanning(&self) -> bool {
        self.job.is_some()
    }

    fn landed(&mut self, update: ScanUpdate) -> bool {
        let (needle, scanned) = match update {
            ScanUpdate::Landed(scanned) => (scanned.needle.clone(), Some(scanned)),
            ScanUpdate::Failed(needle) => (needle, None),
            ScanUpdate::Running(_) | ScanUpdate::Finished => return false,
        };
        let Some(hits) = self.hits.iter_mut().find(|hits| hits.needle == needle) else {
            return false;
        };
        match scanned {
            Some(scanned) => hits.land(scanned),
            None => {
                hits.counted = Counted::default();
                hits.scanned = 0;
            }
        }
        self.rev += 1;
        true
    }

    fn want(&mut self, needle: Needle) {
        match self.hits.iter_mut().find(|hits| hits.needle == needle) {
            Some(hits) => hits.source_changed(None),
            None => self.hits.push(Tally::new(needle, self.layout)),
        }
    }
}

fn mark_update<V: Marked>(view: &mut V, update: ScanUpdate, cx: &mut Context<V>) {
    let marking = view.marking();
    if matches!(update, ScanUpdate::Finished) {
        marking.marks.job = None;
        return start_marks_scan(view, cx);
    }
    if marking.marks.landed(update) {
        marking.rev += 1;
        cx.notify();
    }
}

fn start_marks_scan<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let marking = view.marking();
    if marking.marks.job.is_some() {
        return;
    }
    let scans: Vec<_> = marking
        .marks
        .hits
        .iter_mut()
        .filter_map(Tally::take_scan)
        .collect();
    if scans.is_empty() {
        return;
    }
    let source = marking.source.clone();
    let job = start_scans(source, scans, cx, mark_update::<V>);
    view.marking().marks.job = Some(job);
}

pub(in crate::window_view::viewer) fn recount_marks<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let layer = &mut view.marking().marks;
    let needles: Vec<Needle> = layer.set.iter().map(|mark| mark.needle.clone()).collect();
    layer.hits.retain(|hits| needles.contains(&hits.needle));
    for needle in needles {
        layer.want(needle);
    }
    layer.rev += 1;
    start_marks_scan(view, cx);
}

pub(in crate::window_view::viewer) fn marks_source_changed<V: Marked>(
    view: &mut V,
    appended_to: Option<u64>,
    cx: &mut Context<V>,
) {
    let layer = &mut view.marking().marks;
    if appended_to.is_none() {
        layer.job = None;
    }
    layer
        .hits
        .iter_mut()
        .for_each(|hits| hits.source_changed(appended_to));
    start_marks_scan(view, cx);
}

fn mark_error(error: &MarkError) -> String {
    match error {
        MarkError::Full => "A file holds at most 32 marks. Ctrl+0 clears them.".to_owned(),
        other => other.to_string(),
    }
}

pub(in crate::window_view::viewer) fn toggle_mark<V: Marked>(
    view: &mut V,
    needle: Needle,
    colour: u8,
    cx: &mut Context<V>,
) -> Result<(), String> {
    let layer = &mut view.marking().marks;
    if layer.set.colour_of(&needle) == Some(colour) {
        layer.set.remove(&needle);
        layer.hits.retain(|hits| hits.needle != needle);
    } else {
        let is_new = layer.set.colour_of(&needle).is_none();
        layer
            .set
            .set(needle.clone(), colour)
            .map_err(|error| mark_error(&error))?;
        if is_new {
            view.marking().marks.want(needle);
            start_marks_scan(view, cx);
        }
    }
    marks_changed(view, cx);
    Ok(())
}

pub(in crate::window_view::viewer) fn clear_marks<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let layer = &mut view.marking().marks;
    layer.set.clear();
    layer.hits.clear();
    layer.job = None;
    marks_changed(view, cx);
}

pub(in crate::window_view::viewer) fn marks_changed<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let marking = view.marking();
    marking.rev += 1;
    marking.marks.rev += 1;
    super::memory::memory_changed(view, cx);
    cx.notify();
}

#[cfg(test)]
mod tests {
    use rusty_core::paging::{Counted, Needle};

    use super::MarkLayer;
    use crate::window_view::viewer::search::scan::{ScanUpdate, Scanned};

    fn word() -> Needle {
        Needle::Text {
            text: "needle".to_owned(),
            match_case: false,
        }
    }

    #[test]
    fn only_a_landed_scan_changes_the_layer() {
        let mut layer = MarkLayer::default();
        layer.want(word());
        assert!(!layer.landed(ScanUpdate::Running(40)));
        assert!(!layer.landed(ScanUpdate::Finished));
        assert_eq!(layer.rev, 0);
        let scanned = Scanned {
            needle: word(),
            from: 0,
            len: 100,
            counted: Counted {
                total: 1,
                starts: vec![7],
                done: true,
            },
        };
        assert!(layer.landed(ScanUpdate::Landed(scanned)));
        assert_eq!(layer.rev, 1);
    }

    #[test]
    fn a_failed_scan_makes_the_next_one_start_from_zero() {
        let mut layer = MarkLayer::default();
        layer.want(word());
        let _ = layer.hits[0].take_scan();
        let scanned = Scanned {
            needle: word(),
            from: 0,
            len: 100,
            counted: Counted {
                total: 1,
                starts: vec![7],
                done: true,
            },
        };
        assert!(layer.landed(ScanUpdate::Landed(scanned)));
        assert!(layer.landed(ScanUpdate::Failed(word())));
        layer.hits[0].source_changed(Some(150));
        assert_eq!(layer.hits[0].take_scan().map(|scan| scan.from), Some(0));
    }
}
