use super::build::{Walk, is_copy_like};
use super::{ItemKind, OpKind, PlanRequest, Risk, Step, Totals, is_protected};

const MANY_ITEMS: u64 = 100;
const LARGE_BYTES: u64 = 1 << 30;

pub(super) fn totals(request: &PlanRequest, steps: &[Step], moved: &[bool]) -> Totals {
    let whole_items = matches!(request.kind, OpKind::Recycle)
        || (request.kind == OpKind::Move && request.same_volume);
    if whole_items {
        return scanned_totals(request, moved);
    }
    let count = |wanted: fn(&Step) -> bool| steps.iter().filter(|s| wanted(s)).count() as u64;
    match request.kind {
        OpKind::Delete => Totals {
            files: count(|s| matches!(s, Step::RemoveFile { .. })),
            folders: count(|s| matches!(s, Step::RemoveFolder { .. })),
            bytes: request
                .items
                .iter()
                .filter(|i| i.kind == ItemKind::File)
                .map(|i| i.size)
                .sum(),
        },
        _ => Totals {
            files: count(|s| matches!(s, Step::CopyFile { .. })),
            folders: count(|s| matches!(s, Step::MakeFolder { .. })),
            bytes: steps
                .iter()
                .map(|s| match s {
                    Step::CopyFile { size, .. } => *size,
                    _ => 0,
                })
                .sum(),
        },
    }
}

fn scanned_totals(request: &PlanRequest, moved: &[bool]) -> Totals {
    let mut totals = Totals {
        files: 0,
        folders: 0,
        bytes: 0,
    };
    let mut top_moved = false;
    for (item, moved) in request.items.iter().zip(moved) {
        if item.depth == 0 {
            top_moved = *moved;
        }
        if !top_moved {
            continue;
        }
        match item.kind {
            ItemKind::Folder => totals.folders += 1,
            ItemKind::File => {
                totals.files += 1;
                totals.bytes += item.size;
            }
            ItemKind::Link => {}
        }
    }
    totals
}

pub(super) fn risks(request: &PlanRequest, walk: &Walk<'_>, totals: &Totals) -> Vec<Risk> {
    let mut risks = Vec::new();
    if request.kind == OpKind::Delete {
        risks.push(Risk::Permanent);
    }
    for location in protected_locations(request, walk) {
        let risk = Risk::Protected(location);
        if !risks.contains(&risk) {
            risks.push(risk);
        }
    }
    let touched = match request.kind {
        OpKind::Recycle => totals.files + totals.folders,
        OpKind::Move if request.same_volume => totals.files + totals.folders,
        _ => walk.steps.len() as u64,
    };
    if touched > MANY_ITEMS {
        risks.push(Risk::ManyItems(touched));
    }
    if totals.bytes > LARGE_BYTES {
        risks.push(Risk::Large(totals.bytes));
    }
    if is_copy_like(&request.kind) && walk.links_skipped > 0 {
        risks.push(Risk::LinksSkipped(walk.links_skipped));
    }
    risks
}

fn protected_locations(request: &PlanRequest, walk: &Walk<'_>) -> Vec<crate::Location> {
    let extra = &request.protected;
    let sources = request
        .items
        .iter()
        .map(|i| &i.location)
        .filter(|l| is_protected(l, extra));
    let target = request.target.iter().filter(|l| is_protected(l, extra));
    sources
        .chain(target)
        .chain(walk.into_itself.iter())
        .cloned()
        .collect()
}
