//! Plan targets: the places a plan may overwrite, hashed once per job.
//! Copy, move and rename steps ask `overwrites` in constant time instead of scanning the plan.

use std::collections::HashSet;

use navo_core::{
    Location, OperationPlan, ProviderId, ProviderPath, Resolution, Step, WindowsFoldKey,
};

/// A hashable form of a stored place: a probe of a target equals it exactly when
/// `stored.same_place(target)` holds.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) enum Key {
    /// A local Windows-style place, compared without case.
    Folded(ProviderId, WindowsFoldKey),
    /// Any other place, compared by provider and exact path.
    Exact(ProviderId, ProviderPath),
}

/// Builds the key of a place the plan stores; its own text picks the comparison, as in `same_place`.
pub(super) fn key(stored: &Location) -> Key {
    let provider = stored.provider().clone();
    if stored.folds_case() {
        Key::Folded(provider, stored.path().windows_fold_key())
    } else {
        Key::Exact(provider, stored.path().clone())
    }
}

/// The keys a target may match: one per comparison a stored place can use.
pub(super) fn probes(target: &Location) -> [Key; 2] {
    let provider = target.provider().clone();
    [
        Key::Exact(provider.clone(), target.path().clone()),
        Key::Folded(provider, target.path().windows_fold_key()),
    ]
}

/// The overwrite conflicts and the `WriteFile` targets of one plan, as hash sets.
pub(super) struct PlanTargets {
    overwrite: HashSet<Key>,
    write_file: HashSet<Key>,
}

impl PlanTargets {
    /// Collects the plan's overwrite and write-file targets; costs one pass over the plan.
    pub(super) fn of(plan: &OperationPlan) -> Self {
        let overwrite = plan
            .conflicts
            .iter()
            .filter(|c| c.resolution == Resolution::Overwrite)
            .map(|c| key(&c.target))
            .collect();
        let write_file = plan
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::WriteFile { target, .. } => Some(key(target)),
                _ => None,
            })
            .collect();
        Self {
            overwrite,
            write_file,
        }
    }

    /// True when the plan replaces `target`: an overwrite conflict or a `WriteFile` step names it.
    pub(super) fn overwrites(&self, target: &Location) -> bool {
        probes(target)
            .iter()
            .any(|probe| self.overwrite.contains(probe) || self.write_file.contains(probe))
    }
}

#[cfg(test)]
mod tests {
    use navo_core::{Conflict, FileStamp, OpKind, Totals};
    use proptest::prelude::*;

    use super::*;
    use crate::job::copy::overwrites as oracle;

    // Prefixes mix drive case, separators, UNC forms, a non-ASCII first letter and other providers.
    const PREFIXES: [&str; 10] = [
        r"C:\",
        r"c:\",
        "C:/",
        r"\\srv\share\",
        "//srv/share/",
        "/",
        "\u{17F}:\\",
        r"S:\",
        "s:/",
        "mem://",
    ];
    const NAMES: [&str; 8] = ["a", "A", "\u{E4}", "\u{C4}", "\u{131}", "I", "ss", "SS"];

    /// Builds a location from a prefix, two names and a separator choice.
    fn place((prefix, first, second, slash): (usize, usize, usize, bool)) -> Location {
        let separator = if slash { '/' } else { '\\' };
        let text = format!(
            "{}{}{separator}{}",
            PREFIXES[prefix], NAMES[first], NAMES[second]
        );
        Location::new(text).expect("generated places are valid")
    }

    fn places() -> impl Strategy<Value = Location> {
        (
            0..PREFIXES.len(),
            0..NAMES.len(),
            0..NAMES.len(),
            any::<bool>(),
        )
            .prop_map(place)
    }

    fn resolution() -> impl Strategy<Value = Resolution> {
        prop_oneof![
            Just(Resolution::Skip),
            Just(Resolution::Overwrite),
            Just(Resolution::Ask),
            places().prop_map(Resolution::Rename),
        ]
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            (places(), places()).prop_map(|(target, staged)| Step::WriteFile {
                target,
                staged,
                staged_len: 1,
                expect: FileStamp {
                    size: 1,
                    modified_ms: None,
                    hash: [0; 32],
                },
            }),
            (places(), places()).prop_map(|(source, target)| Step::CopyFile {
                source,
                target,
                size: 1
            }),
            (places(), places()).prop_map(|(source, target)| Step::Rename { source, target }),
        ]
    }

    fn plans() -> impl Strategy<Value = OperationPlan> {
        let conflicts = prop::collection::vec((places(), places(), resolution()), 0..8);
        (prop::collection::vec(step(), 0..8), conflicts).prop_map(|(steps, conflicts)| {
            OperationPlan {
                kind: OpKind::Copy,
                steps,
                totals: Totals {
                    files: 0,
                    folders: 0,
                    bytes: 0,
                },
                conflicts: conflicts
                    .into_iter()
                    .map(|(source, target, resolution)| Conflict {
                        source,
                        target,
                        resolution,
                    })
                    .collect(),
                risks: Vec::new(),
            }
        })
    }

    /// Every target the plan names, plus free probes, so both hits and misses are checked.
    fn targets(plan: &OperationPlan, extra: Vec<Location>) -> Vec<Location> {
        let in_steps = plan.steps.iter().filter_map(|step| match step {
            Step::WriteFile { target, .. }
            | Step::CopyFile { target, .. }
            | Step::Rename { target, .. } => Some(target.clone()),
            _ => None,
        });
        let in_conflicts = plan.conflicts.iter().map(|c| c.target.clone());
        in_steps.chain(in_conflicts).chain(extra).collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn plan_targets_answer_like_the_plan_scan(
            plan in plans(),
            extra in prop::collection::vec(places(), 0..8),
        ) {
            let hashed = PlanTargets::of(&plan);
            for target in targets(&plan, extra) {
                prop_assert_eq!(hashed.overwrites(&target), oracle(&plan, &target), "{}", target.as_str());
            }
        }
    }
}
