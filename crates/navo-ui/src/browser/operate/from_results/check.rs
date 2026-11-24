//! Background planning for F5/F6 from Find results: checks each row against its search-time
//! stamp, folds aliases and repeats, picks rename or copy per volume and refuses any write that
//! would land on or over a selected source.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use navo_core::{EntryKind, FileIdentity, FsError, FsErrorKind, Location, OperationPlan, Step};
use navo_fs::{CancelToken, ProviderOps, volume_of};
use navo_l10n::FluentArgs;

use super::super::OpRequest;
use super::RowStamp;
use super::alias::{Aliases, Place, Unresolved, is_partial, place};

/// A kept row: the location the plan uses and its resolved place.
struct Top {
    location: Location,
    stamp: RowStamp,
    real: Location,
    /// The entry's file identity, when the provider has one: tells case variants apart.
    identity: Option<FileIdentity>,
}

/// Plans a results request: refuses partial paths, changed rows, targets inside a selected
/// folder and writes over a selected source, each by name. Leaves `request` holding the kept
/// rows and their stamps, so a rebuilt plan checks them again.
pub(in crate::browser::operate) fn plan_rows(
    provider: &dyn ProviderOps,
    request: &mut OpRequest,
    stamps: &[RowStamp],
    cancel: &CancelToken,
) -> Result<OperationPlan, String> {
    let target = request.target.clone().ok_or_else(nothing)?;
    if stamps.len() != request.sources.len() {
        return Err(nothing());
    }
    if is_partial(&target) {
        return Err(said("results-partial-path", &target, None));
    }
    let mut aliases = Aliases::new(provider);
    let real_target = aliases.real(&target).map_err(unresolved)?;
    let tops = distinct_tops(provider, &mut aliases, (&request.sources, stamps), cancel)?;
    let tops_at: HashSet<Place> = tops.iter().map(|top| place(&top.real)).collect();
    if let Some(inside) = std::iter::successors(Some(real_target.clone()), Location::parent)
        .find(|place_of| tops_at.contains(&place(place_of)))
    {
        return Err(said("results-into-source", &target, Some(&inside)));
    }
    request.same_volume = all_on_volume(provider, &tops, &real_target);
    if let Some(structure) = request.structure.as_mut() {
        structure.same_volume = request.same_volume;
    }
    request.results = Some(tops.iter().map(|top| top.stamp).collect::<Arc<[_]>>());
    request.sources = tops.iter().map(|top| top.location.clone()).collect();
    let plan = super::super::work::plan_for(provider, request, cancel).map_err(|e| e.message())?;
    refuse_overwrites(&mut aliases, &plan, &tops)?;
    Ok(plan)
}

/// The rows in first-seen order without repeats or rows below another row, each checked
/// against its search-time stamp; aliases count as the place they name.
fn distinct_tops(
    provider: &dyn ProviderOps,
    aliases: &mut Aliases<'_>,
    (sources, stamps): (&[Location], &[RowStamp]),
    cancel: &CancelToken,
) -> Result<Vec<Top>, String> {
    let mut rows = Vec::with_capacity(sources.len());
    for (location, stamp) in sources.iter().zip(stamps) {
        if cancel.is_cancelled() {
            return Err(FsError::new(FsErrorKind::Cancelled).message());
        }
        if is_partial(location) {
            return Err(said("results-partial-path", location, None));
        }
        let real = aliases.real_entry(location).map_err(unresolved)?;
        let entry = provider.metadata(&real).map_err(|error| error.message())?;
        if !stamp.matches(&RowStamp::of(&entry)) {
            return Err(said("results-changed", location, None));
        }
        rows.push(Top {
            location: location.clone(),
            stamp: *stamp,
            identity: provider.file_identity(&real).ok().flatten(),
            real,
        });
    }
    Ok(distinct(rows, &mut |folder| {
        provider.file_identity(folder).ok().flatten()
    }))
}

/// `rows` without repeats or rows below another row. Case variants of one name count as one
/// place unless their file identities differ; `identity_of` reads a folder's identity when a
/// row's folder is a case variant of another row.
fn distinct(
    rows: Vec<Top>,
    identity_of: &mut dyn FnMut(&Location) -> Option<FileIdentity>,
) -> Vec<Top> {
    let mut at: HashMap<Place, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        at.entry(place(&row.real)).or_default().push(index);
    }
    let covered: Vec<bool> = rows
        .iter()
        .map(|row| {
            std::iter::successors(row.real.parent(), Location::parent)
                .any(|folder| is_row(&rows, at.get(&place(&folder)), &folder, identity_of))
        })
        .collect();
    let mut seen: HashMap<Place, Vec<Option<FileIdentity>>> = HashMap::new();
    let mut kept = Vec::with_capacity(rows.len());
    for (row, covered) in rows.into_iter().zip(covered) {
        let twins = seen.entry(place(&row.real)).or_default();
        if covered || twins.iter().any(|twin| !differ(*twin, row.identity)) {
            continue;
        }
        twins.push(row.identity);
        kept.push(row);
    }
    kept
}

/// True when one of the rows `found` at `folder`'s place is that folder: the same name, or a
/// case variant whose identity does not differ.
fn is_row(
    rows: &[Top],
    found: Option<&Vec<usize>>,
    folder: &Location,
    identity_of: &mut dyn FnMut(&Location) -> Option<FileIdentity>,
) -> bool {
    found
        .into_iter()
        .flatten()
        .filter_map(|&index| rows.get(index))
        .any(|row| row.real == *folder || !differ(row.identity, identity_of(folder)))
}

/// True when both identities are known and differ: two case variants are two entries.
fn differ(a: Option<FileIdentity>, b: Option<FileIdentity>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a != b)
}

impl RowStamp {
    /// Same kind, and for a file the same size and time; folder times change with their content.
    fn matches(&self, now: &Self) -> bool {
        self.kind == now.kind && (self.kind != EntryKind::File || self == now)
    }
}

/// True when every row and the target share a volume, so a move may rename. Row order never
/// decides: one row on another drive makes every row copy, then remove.
fn all_on_volume(provider: &dyn ProviderOps, tops: &[Top], target: &Location) -> bool {
    let serial = |location: &Location| provider.volume_serial(location).ok().flatten();
    let target_serial = serial(target);
    let mut folders: HashMap<Place, Option<u64>> = HashMap::new();
    tops.iter().all(|top| {
        let folder = top.real.parent().unwrap_or_else(|| top.real.clone());
        let source_serial = *folders
            .entry(place(&folder))
            .or_insert_with(|| serial(&folder));
        let known = source_serial.zip(target_serial);
        volume_of(&top.real) == volume_of(target) && known.is_none_or(|(a, b)| a == b)
    })
}

/// Refuses the plan when a write or a listed conflict lands on a selected source, inside a
/// selected folder, or on a folder that holds a selected source.
fn refuse_overwrites(
    aliases: &mut Aliases<'_>,
    plan: &OperationPlan,
    tops: &[Top],
) -> Result<(), String> {
    let tops_at: HashSet<Place> = tops.iter().map(|top| place(&top.real)).collect();
    let holders: HashSet<Place> = tops
        .iter()
        .flat_map(|top| std::iter::successors(top.real.parent(), Location::parent))
        .map(|folder| place(&folder))
        .collect();
    let writes = plan.steps.iter().filter_map(written);
    let conflicts = plan.conflicts.iter().map(|c| (Some(&c.source), &c.target));
    for (source, target) in writes.chain(conflicts) {
        if source.is_some_and(|source| source.same_place(target)) {
            continue;
        }
        let real = aliases.real_entry(target).map_err(unresolved)?;
        let lands_on = holders.contains(&place(&real))
            || std::iter::successors(Some(real), Location::parent)
                .any(|place_of| tops_at.contains(&place(&place_of)));
        if lands_on {
            return Err(said("results-overwrites-source", target, None));
        }
    }
    Ok(())
}

/// The place a step writes and the source it copies from, for steps that write a name.
fn written(step: &Step) -> Option<(Option<&Location>, &Location)> {
    match step {
        Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
            Some((Some(source), target))
        }
        Step::WriteFile { target, .. } => Some((None, target)),
        Step::MakeLink { at, .. } => Some((None, at)),
        _ => None,
    }
}

fn unresolved(reason: Unresolved) -> String {
    match reason {
        Unresolved::Partial(location) => said("results-partial-path", &location, None),
        Unresolved::Loop(location) => said("results-link-loop", &location, None),
        Unresolved::Unreadable(location) => said("results-link-unreadable", &location, None),
    }
}

fn nothing() -> String {
    FsError::new(FsErrorKind::NotFound).message()
}

/// The Fluent message `id` naming `location` and, when given, the selected folder `inside`.
fn said(id: &str, location: &Location, inside: Option<&Location>) -> String {
    let mut args = FluentArgs::new();
    args.set("path", location.path().display().into_owned());
    if let Some(inside) = inside {
        args.set("inside", inside.path().display().into_owned());
    }
    navo_l10n::tr_args(id, &args)
}

#[cfg(test)]
mod tests {
    use navo_core::{FileIdentityKind, ProviderPath};

    use super::*;

    fn top(text: &str, file: Option<u8>) -> Top {
        let location = Location::local(ProviderPath::from_text(text)).expect("a location");
        Top {
            location: location.clone(),
            stamp: RowStamp {
                kind: EntryKind::File,
                size: 1,
                modified_ms: None,
            },
            real: location,
            identity: file.map(|file| FileIdentity {
                volume: 1,
                file: [file; 16],
                kind: FileIdentityKind::Native,
            }),
        }
    }

    fn texts(rows: &[Top]) -> Vec<&str> {
        rows.iter().map(|row| row.location.as_str()).collect()
    }

    #[test]
    fn case_variants_with_different_identities_stay_apart() {
        let rows = vec![
            top(r"C:\w\A.c", Some(1)),
            top(r"C:\w\a.c", Some(2)),
            top(r"C:\w\A.C", Some(1)),
            top(r"C:\w\b.c", None),
            top(r"c:\W\B.c", None),
        ];
        let kept = distinct(rows, &mut |_| None);
        assert_eq!(texts(&kept), [r"C:\w\A.c", r"C:\w\a.c", r"C:\w\b.c"]);
    }

    #[test]
    fn a_row_in_a_case_variant_of_a_selected_folder_stays() {
        let rows = vec![
            top(r"C:\w\D", Some(1)),
            top(r"C:\w\d\x.c", Some(3)),
            top(r"C:\w\D\y.c", Some(4)),
        ];
        let other = Location::local(ProviderPath::from_text(r"C:\w\d")).expect("a location");
        let kept = distinct(rows, &mut |folder| {
            (*folder == other).then_some(FileIdentity {
                volume: 1,
                file: [2; 16],
                kind: FileIdentityKind::Native,
            })
        });
        assert_eq!(texts(&kept), [r"C:\w\D", r"C:\w\d\x.c"]);
    }
}
