use std::time::Duration;

use rusty_core::paging::{Counted, Needle, count, ordinal};

use super::jobs::{Job, Throttle, spawn_job};
use super::{Marked, Marking};
use crate::format;
use crate::prelude::*;
use crate::window_view::viewer::files::Reopen;

pub(in crate::window_view::viewer) const KEEP: usize = 100_000;
const COUNT_EVERY: Duration = Duration::from_millis(100);

pub(in crate::window_view::viewer) enum CountUpdate {
    Running(u64),
    Done(Counted),
    Failed,
}

pub(in crate::window_view::viewer) struct CountRun {
    pub needle: Needle,
    pub counted: Counted,
    is_recount: bool,
    _job: Job,
}

pub(in crate::window_view::viewer) fn needle_width(needle: &Needle) -> u64 {
    match needle {
        Needle::Text { text, .. } => text.len() as u64,
        Needle::Bytes(bytes) => bytes.len() as u64,
    }
}

pub(in crate::window_view::viewer) fn count_label(counted: &Counted, hit: Option<u64>) -> String {
    let total = format::bytes(counted.total);
    if !counted.done {
        return format!("… of {total}");
    }
    if counted.total == 0 {
        return "No matches".to_owned();
    }
    match hit.and_then(|at| ordinal(counted, at)) {
        Some(place) => format!("{} of {total}", format::bytes(place)),
        None => format!("of {total}"),
    }
}

pub(in crate::window_view::viewer) fn start_count<V: 'static>(
    source: Reopen,
    needle: Needle,
    cx: &mut Context<V>,
    apply: impl Fn(&mut V, CountUpdate, &mut Context<V>) + 'static,
) -> Job {
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(COUNT_EVERY);
            let counted = source().and_then(|source| {
                count(&*source, &needle, KEEP, cancel, &mut |_, total| {
                    throttle.when_due(|| send(CountUpdate::Running(total)));
                })
            });
            match counted {
                Ok(counted) if counted.done => send(CountUpdate::Done(counted)),
                Ok(_) => {}
                Err(_) => send(CountUpdate::Failed),
            }
        },
        apply,
    )
}

impl Marking {
    pub(in crate::window_view::viewer) fn counted(&self) -> Option<&Counted> {
        self.count.as_ref().map(|run| &run.counted)
    }

    fn count_update(&mut self, update: CountUpdate) {
        let Some(run) = &mut self.count else {
            return;
        };
        match update {
            CountUpdate::Running(_) if run.is_recount => {}
            CountUpdate::Running(total) => run.counted.total = total,
            CountUpdate::Done(counted) => {
                run.counted = counted;
                self.rev += 1;
            }
            CountUpdate::Failed => {
                self.count = None;
                self.rev += 1;
            }
        }
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
        .is_some_and(|run| run.needle == needle)
    {
        return;
    }
    restart_count(view, needle, cx);
}

pub(in crate::window_view::viewer) fn restart_count<V: Marked>(
    view: &mut V,
    needle: Needle,
    cx: &mut Context<V>,
) {
    let source = view.marking().source.clone();
    let job = start_count(source, needle.clone(), cx, |view: &mut V, update, cx| {
        view.marking().count_update(update);
        cx.notify();
    });
    let marking = view.marking();
    let earlier = marking
        .count
        .take()
        .filter(|run| run.needle == needle && run.counted.done)
        .map(|run| run.counted);
    marking.count = Some(CountRun {
        needle,
        is_recount: earlier.is_some(),
        counted: earlier.unwrap_or_default(),
        _job: job,
    });
    marking.rev += 1;
}

#[cfg(test)]
mod tests {
    use rusty_core::paging::Counted;

    use super::count_label;

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
        assert_eq!(count_label(&counted, Some(500)), "of 1,248");
        assert_eq!(count_label(&counted, None), "of 1,248");
    }

    #[test]
    fn no_matches_says_so() {
        assert_eq!(
            count_label(&counted(0, Vec::new(), true), None),
            "No matches"
        );
    }
}
