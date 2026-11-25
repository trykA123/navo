//! Lookups a crash rollback builds once from its journal (MR1f). Each step asks "done?", "held or
//! placed row?" and "removed by this job?" with a hash probe, so a rollback stays linear in size.

use std::collections::HashMap;
use std::ops::Range;

use navo_core::{Location, Step};

use super::done_set::DoneSet;
use super::plan_targets::{Key, key, probes};
use super::undo::{compared, scanned};
use super::{HeldFile, Journaled, PlacedFile};

/// Row numbers keyed by the place each row names: a probe finds exactly the rows whose place
/// `same_place`s it, since each row's own text picks its comparison as in `same_place`.
struct Places(HashMap<Key, Vec<usize>>);

impl Places {
    fn of<'l>(places: impl Iterator<Item = Option<&'l Location>>) -> Self {
        let mut map: HashMap<Key, Vec<usize>> = HashMap::new();
        for (row, place) in places.enumerate() {
            if let Some(place) = place {
                map.entry(key(place)).or_default().push(row);
            }
        }
        Self(map)
    }

    /// The rows at the same place as `probe`, ascending. A row sits under one key only, so the
    /// two probes never find it twice.
    fn matching(&self, probe: &Location) -> Vec<usize> {
        compared();
        let mut rows: Vec<usize> = probes(probe)
            .iter()
            .filter_map(|probe| self.0.get(probe))
            .flatten()
            .copied()
            .inspect(|_| compared())
            .collect();
        rows.sort_unstable();
        rows
    }
}

/// A journaled job under rollback with its lookups, built once in [`Rollback::of`].
pub(super) struct Rollback<'j> {
    pub(super) job: &'j Journaled<'j>,
    done: DoneSet,
    held: Places,
    placed: Places,
    removals: Places,
}

impl<'j> Rollback<'j> {
    /// Indexes the job's done seqs, held and placed rows and source removals in one pass each.
    pub(super) fn of(job: &'j Journaled<'j>) -> Self {
        let removals = job.plan.steps.iter().map(|step| match step {
            Step::RemoveFile { location } => Some(location),
            _ => None,
        });
        Self {
            job,
            done: DoneSet::of_plan(job.plan, job.done),
            held: Places::of(job.held.iter().map(|file| Some(&file.original))),
            placed: Places::of(job.placed.iter().map(|file| Some(&file.target))),
            removals: Places::of(removals),
        }
    }

    /// True when the journal marks step `seq` done.
    pub(super) fn is_done(&self, seq: u32) -> bool {
        compared();
        self.done.contains(&seq)
    }

    /// The held row for `target`: the first naming it exactly, else the first at the same place.
    pub(super) fn held_for(&self, target: &Location) -> Option<&'j HeldFile> {
        first_of(&self.held, self.job.held, target, |file| &file.original)
    }

    /// The placed row for `target`: the first naming it exactly, else the first at the same place.
    pub(super) fn placed_for(&self, target: &Location) -> Option<&'j PlacedFile> {
        first_of(&self.placed, self.job.placed, target, |file| &file.target)
    }

    /// The newest placed row at the same place as `target`: a retried copy journals again.
    pub(super) fn newest_placed(&self, target: &Location) -> Option<&'j PlacedFile> {
        let rows = self.placed.matching(target);
        rows.last().and_then(|row| self.job.placed.get(*row))
    }

    /// True when a source removal step for `source` is marked done.
    pub(super) fn removal_done(&self, source: &Location) -> bool {
        self.removals
            .matching(source)
            .into_iter()
            .any(|seq| u32::try_from(seq).is_ok_and(|seq| self.is_done(seq)))
    }

    /// True when the plan removes `source` at a seq inside `window`. An empty window probes nothing.
    pub(super) fn removes_within(&self, source: &Location, window: &Range<u32>) -> bool {
        if window.is_empty() {
            return false;
        }
        self.removals.matching(source).into_iter().any(|seq| {
            scanned();
            u32::try_from(seq).is_ok_and(|seq| window.contains(&seq))
        })
    }
}

/// The first of `rows` whose place equals `target` exactly, else the first at the same place.
/// An exact match is also a same-place match, so both come from one probe.
fn first_of<'r, T>(
    places: &Places,
    rows: &'r [T],
    target: &Location,
    place: impl Fn(&T) -> &Location,
) -> Option<&'r T> {
    let found: Vec<&T> = places
        .matching(target)
        .into_iter()
        .filter_map(|row| rows.get(row))
        .collect();
    let exact = found.iter().find(|row| place(row) == target);
    exact.or_else(|| found.first()).copied()
}

#[cfg(test)]
mod tests {
    use navo_core::{OpKind, OperationPlan, Totals};
    use proptest::prelude::*;

    use super::super::FileStamp;
    use super::*;

    // Drive case, separators, UNC, a non-ASCII first letter (folds one way only) and a provider.
    const PREFIXES: [&str; 7] = [
        r"C:\",
        r"c:\",
        "C:/",
        r"\\srv\share\",
        "/",
        "\u{17F}:\\",
        "mem://",
    ];
    const NAMES: [&str; 6] = ["a", "A", "\u{E4}", "\u{C4}", "ss", "SS"];

    fn places() -> impl Strategy<Value = Location> {
        (0..PREFIXES.len(), 0..NAMES.len()).prop_map(|(prefix, name)| {
            Location::new(format!("{}{}", PREFIXES[prefix], NAMES[name])).expect("valid place")
        })
    }

    fn placed(target: Location, size: u64) -> PlacedFile {
        let stamp = FileStamp {
            size,
            modified_ms: None,
        };
        PlacedFile {
            target,
            stamp,
            source: None,
        }
    }

    fn held(original: Location) -> HeldFile {
        HeldFile {
            held: original.clone(),
            original,
            placed: None,
            volume: None,
            placed_hash: None,
        }
    }

    fn plan(removed: &[Location]) -> OperationPlan {
        let steps = removed.iter().map(|location| Step::RemoveFile {
            location: location.clone(),
        });
        let totals = Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        };
        OperationPlan {
            kind: OpKind::Move,
            steps: steps.collect(),
            totals,
            conflicts: Vec::new(),
            risks: Vec::new(),
        }
    }

    proptest! {
        /// Every lookup answers what the per-copy scans it replaced answered, row for row.
        #[test]
        fn lookups_agree_with_the_scans_they_replace(
            rows in prop::collection::vec(places(), 0..12),
            probe in places(),
            done in prop::collection::vec(0_u32..12, 0..12),
            window in (0_u32..14, 0_u32..14),
        ) {
            let placed_rows: Vec<PlacedFile> =
                (0_u64..).zip(&rows).map(|(at, row)| placed(row.clone(), at)).collect();
            let held_rows: Vec<HeldFile> = rows.iter().cloned().map(held).collect();
            let plan = plan(&rows);
            let job = Journaled {
                plan: &plan,
                done: &done,
                held: &held_rows,
                placed: &placed_rows,
                held_unreadable: false,
                unsure: None,
                unmarked_removals: None,
            };
            let run = Rollback::of(&job);
            let held_scan = held_rows.iter().find(|file| file.original == probe)
                .or_else(|| held_rows.iter().find(|file| file.original.same_place(&probe)));
            prop_assert_eq!(run.held_for(&probe), held_scan);
            let placed_scan = placed_rows.iter().find(|file| file.target == probe)
                .or_else(|| placed_rows.iter().find(|file| file.target.same_place(&probe)));
            prop_assert_eq!(run.placed_for(&probe), placed_scan);
            let newest_scan = placed_rows.iter().rev().find(|file| file.target.same_place(&probe));
            prop_assert_eq!(run.newest_placed(&probe), newest_scan);
            let removes = |seq: u32, step: &Step| {
                matches!(step, Step::RemoveFile { location } if location.same_place(&probe))
                    .then_some(seq)
            };
            let removed: Vec<u32> = (0_u32..)
                .zip(&plan.steps)
                .filter_map(|(seq, step)| removes(seq, step))
                .collect();
            prop_assert_eq!(run.removal_done(&probe), removed.iter().any(|seq| done.contains(seq)));
            let window = window.0..window.1;
            let end = (window.end as usize).min(plan.steps.len());
            let in_window = plan.steps.get(window.start as usize..end).unwrap_or_default();
            let window_scan = in_window.iter().any(|step| removes(0, step).is_some());
            prop_assert_eq!(run.removes_within(&probe, &window), window_scan);
        }
    }
}
