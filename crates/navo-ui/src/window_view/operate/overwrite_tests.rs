//! AD15: a copy or move that would replace files on a drive that cannot keep them shows the
//! same kind of warning line as a permanent delete there; Skip and Keep both never show it.

use navo_core::{
    AppState, Conflict, ConflictPolicy, Location, OpKind, OperationPlan, Resolution, Totals,
};

use super::confirm_props;
use crate::browser::{Clash, Facts, KeepOld, NoKeep, PendingOp, Replan};

fn at(path: &str) -> Location {
    Location::new(path).unwrap()
}

/// A file's facts with `modified_ms` as its time.
fn file(modified_ms: i64) -> Facts {
    Facts {
        size: Some(1),
        modified_ms: Some(modified_ms),
        is_folder: false,
    }
}

/// A copy of `/home/x.txt` into `/dest` where `x.txt` is already there; the plan resolved it as
/// `resolution` under `planned`, the user now picks `picked`, and the source is `newer` or not.
fn copy(
    planned: ConflictPolicy,
    picked: ConflictPolicy,
    resolution: Resolution,
    newer: bool,
) -> PendingOp {
    let plan = OperationPlan {
        kind: OpKind::Copy,
        steps: Vec::new(),
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: 1,
        },
        conflicts: vec![Conflict {
            source: at("/home/x.txt"),
            target: at("/dest/x.txt"),
            resolution,
        }],
        risks: Vec::new(),
    };
    let (source, target) = if newer { (20, 10) } else { (10, 20) };
    PendingOp {
        id: 1,
        sources: vec![at("/home/x.txt")],
        target: Some(at("/dest")),
        plan,
        retain_deletes: None,
        cannot_retain: Some(NoKeep::Drive),
        keep_old: KeepOld::AsSet,
        source_tab: AppState::new(at("/home")).active_tab().id(),
        undo: None,
        slow_volumes: None,
        structure: None,
        stream: None,
        replan: Replan {
            policy: picked,
            planned,
            same_volume: false,
        },
        clashes: vec![Clash {
            name: "x.txt".to_owned(),
            source: file(source),
            target: file(target),
        }],
    }
}

/// A plan built and shown for `policy`, resolving the conflict as `resolution`.
fn settled(policy: ConflictPolicy, resolution: Resolution, newer: bool) -> PendingOp {
    copy(policy.clone(), policy, resolution, newer)
}

fn warns(pending: &PendingOp) -> bool {
    let line = navo_l10n::tr("overwrite-cannot-retain");
    confirm_props(pending).summary.contains(line.as_str())
}

#[test]
fn overwrite_all_on_a_drive_that_cannot_keep_files_warns() {
    let pending = settled(ConflictPolicy::OverwriteAll, Resolution::Overwrite, false);
    assert!(warns(&pending), "{}", confirm_props(&pending).summary);
    let summary = confirm_props(&pending).summary;
    let (sizes, wrapped) = summary.split_once('\n').expect("a conflict line");
    let line = navo_l10n::tr("overwrite-cannot-retain");
    assert!(
        !sizes.contains(line.as_str()),
        "the cut first line hides it"
    );
    assert!(wrapped.contains(line.as_str()));
    let keeping = PendingOp {
        cannot_retain: None,
        ..pending
    };
    assert!(!warns(&keeping));
}

#[test]
fn skip_and_keep_both_never_warn() {
    assert!(!warns(&settled(
        ConflictPolicy::Skip,
        Resolution::Skip,
        true
    )));
    let renamed = Resolution::Rename(at("/dest/x (2).txt"));
    assert!(!warns(&settled(ConflictPolicy::Rename, renamed, true)));
    assert!(!warns(&settled(ConflictPolicy::Ask, Resolution::Ask, true)));
}

#[test]
fn overwrite_older_warns_only_when_a_target_is_older() {
    let none_older = settled(ConflictPolicy::OverwriteOlder, Resolution::Skip, false);
    assert!(!warns(&none_older));
    let older = settled(ConflictPolicy::OverwriteOlder, Resolution::Overwrite, true);
    assert!(warns(&older));
}

#[test]
fn switching_the_choice_updates_the_warning_before_the_plan_is_rebuilt() {
    let to_all = copy(
        ConflictPolicy::Skip,
        ConflictPolicy::OverwriteAll,
        Resolution::Skip,
        false,
    );
    assert!(to_all.is_planning());
    assert!(warns(&to_all));
    let back = copy(
        ConflictPolicy::OverwriteAll,
        ConflictPolicy::Skip,
        Resolution::Overwrite,
        false,
    );
    assert!(!warns(&back));
    let older = |newer| {
        copy(
            ConflictPolicy::Skip,
            ConflictPolicy::OverwriteOlder,
            Resolution::Skip,
            newer,
        )
    };
    assert!(warns(&older(true)));
    assert!(!warns(&older(false)));
}

#[test]
fn a_move_warns_like_a_copy_and_a_delete_keeps_its_own_line() {
    let mut moving = settled(ConflictPolicy::OverwriteAll, Resolution::Overwrite, false);
    moving.plan.kind = OpKind::Move;
    assert!(warns(&moving));
    let mut delete = settled(ConflictPolicy::OverwriteAll, Resolution::Overwrite, false);
    delete.plan.kind = OpKind::Delete;
    assert!(!warns(&delete));
}
