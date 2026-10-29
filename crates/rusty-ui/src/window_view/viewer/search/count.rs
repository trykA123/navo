use std::ops::Range;
use std::time::Duration;

use gpui_kit::Task;
use rusty_core::paging::{Counted, Needle, ordinal};
use rusty_l10n::t;

use super::jobs::Job;
use super::scan::{ScanUpdate, Tally, start_scans};
use super::{Marked, Marking};
use crate::format;
use crate::prelude::*;

const TYPING_PAUSE: Duration = Duration::from_millis(150);

pub(in crate::window_view::viewer) struct CountRun {
    pub tally: Tally,
    pub failed: bool,
    pub generation: u64,
    job: Option<Job>,
}

pub(in crate::window_view::viewer) fn count_label(counted: &Counted, hit: Option<u64>) -> String {
    let total = format::bytes(counted.total);
    if !counted.done {
        return t!("viewer-find-counting", total = total);
    }
    if counted.total == 0 {
        return t!("viewer-find-none");
    }
    match hit.and_then(|at| ordinal(counted, at)) {
        Some(place) => t!(
            "viewer-find-place",
            place = format::bytes(place),
            total = total
        ),
        None => t!("viewer-find-total", count = counted.total, shown = total),
    }
}

impl CountRun {
    pub(in crate::window_view::viewer) fn needle(&self) -> &Needle {
        &self.tally.needle
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn is_scanning(&self) -> bool {
        self.job.is_some()
    }
}

pub(in crate::window_view::viewer) fn ranges_in(
    starts: &[u64],
    width: u64,
    span: Range<u64>,
) -> Vec<Range<u64>> {
    let first = starts.partition_point(|start| start + width <= span.start);
    starts[first..]
        .iter()
        .take_while(|start| **start < span.end)
        .map(|start| *start..start + width)
        .collect()
}

impl Marking {
    pub(in crate::window_view::viewer) fn match_ranges(&self, span: Range<u64>) -> Vec<Range<u64>> {
        match self.count.as_ref().filter(|run| !run.failed) {
            Some(run) => ranges_in(&run.tally.counted.starts, run.tally.width(), span),
            None => Vec::new(),
        }
    }

    pub(in crate::window_view::viewer) fn counted(&self) -> Option<&Counted> {
        self.count
            .as_ref()
            .filter(|run| !run.failed)
            .map(|run| &run.tally.counted)
    }

    fn count_update(&mut self, update: ScanUpdate) -> bool {
        let Some(run) = &mut self.count else {
            return false;
        };
        match update {
            ScanUpdate::Running(total) if !run.tally.counted.done => {
                run.tally.counted.total = total;
            }
            ScanUpdate::Landed(scanned) if scanned.needle == run.tally.needle => {
                run.tally.land(scanned);
                run.generation += 1;
                self.rev += 1;
            }
            ScanUpdate::Failed(needle) if needle == run.tally.needle => {
                run.failed = true;
                self.rev += 1;
            }
            ScanUpdate::Finished => {
                run.job = None;
                return true;
            }
            _ => {}
        }
        false
    }
}

fn start_search_scan<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let marking = view.marking();
    let source = marking.source.clone();
    let Some(run) = marking.count.as_mut().filter(|run| run.job.is_none()) else {
        return;
    };
    let Some(scan) = run.tally.take_scan() else {
        return;
    };
    let job = start_scans(source, vec![scan], cx, |view: &mut V, update, cx| {
        if view.marking().count_update(update) {
            start_search_scan(view, cx);
        }
        cx.notify();
    });
    if let Some(run) = &mut view.marking().count {
        run.job = Some(job);
    }
}

pub(in crate::window_view::viewer) fn count_search<V: Marked>(
    view: &mut V,
    needle: Needle,
    cx: &mut Context<V>,
) {
    let marking = view.marking();
    if marking
        .count
        .as_ref()
        .is_some_and(|run| *run.needle() == needle)
    {
        return;
    }
    marking.count = Some(CountRun {
        tally: Tally::new(needle, marking.layout),
        failed: false,
        generation: 0,
        job: None,
    });
    marking.rev += 1;
    start_search_scan(view, cx);
}

pub(in crate::window_view::viewer) fn count_after_pause<V: Marked>(
    needle_of: fn(&V, &App) -> Option<Needle>,
    cx: &mut Context<V>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(TYPING_PAUSE).await;
        let _ = this.update(cx, |view, cx| {
            match needle_of(view, cx) {
                Some(needle) => count_search(view, needle, cx),
                None => {
                    let marking = view.marking();
                    marking.count = None;
                    marking.rev += 1;
                }
            }
            cx.notify();
        });
    })
}

pub(in crate::window_view::viewer) fn count_source_changed<V: Marked>(
    view: &mut V,
    appended_to: Option<u64>,
    cx: &mut Context<V>,
) {
    if let Some(run) = &mut view.marking().count {
        run.failed = false;
        if appended_to.is_none() {
            run.job = None;
        }
        run.tally.source_changed(appended_to);
    }
    start_search_scan(view, cx);
}

#[cfg(test)]
mod tests {
    use rusty_core::paging::Counted;

    use super::{count_label, ranges_in};

    fn counted(total: u64, starts: Vec<u64>, done: bool) -> Counted {
        Counted {
            total,
            starts,
            done,
        }
    }

    #[test]
    fn a_kept_hit_shows_its_place_and_the_total() {
        let counted = counted(1248, vec![4, 9, 30], true);
        assert_eq!(count_label(&counted, Some(30)), "3 of 1,248");
    }

    #[test]
    fn a_running_count_shows_the_total_so_far() {
        let counted = counted(1248, Vec::new(), false);
        assert_eq!(count_label(&counted, Some(4)), "… of 1,248");
    }

    #[test]
    fn a_hit_past_the_kept_starts_shows_only_the_total() {
        let counted = counted(1248, vec![4, 9], true);
        assert_eq!(count_label(&counted, Some(500)), "1,248 matches");
        assert_eq!(count_label(&counted, None), "1,248 matches");
    }

    #[test]
    fn one_match_is_singular() {
        assert_eq!(count_label(&counted(1, vec![4], true), None), "1 match");
    }

    #[test]
    fn no_matches_says_so() {
        assert_eq!(
            count_label(&counted(0, Vec::new(), true), None),
            "No matches"
        );
    }

    #[test]
    fn ranges_in_a_span_keep_the_matches_that_reach_into_it() {
        let starts = [2, 10, 20, 30];
        assert_eq!(ranges_in(&starts, 4, 12..30), [10..14, 20..24]);
        assert_eq!(
            ranges_in(&starts, 4, 14..20),
            Vec::<std::ops::Range<u64>>::new()
        );
        assert_eq!(ranges_in(&starts, 4, 0..3).first(), Some(&(2..6)));
    }
}
