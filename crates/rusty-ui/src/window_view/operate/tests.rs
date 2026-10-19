use rusty_core::{AppState, Conflict, Location, OpKind, OperationPlan, Resolution, Risk, Totals};
use rusty_fs::{CancelToken, Progress};

use super::super::jobs::job_info;
use super::{confirm_props, mode_label};
use crate::browser::{JobStatus, JobView, PendingOp};
use crate::job_row::JobRowState;
use rusty_fs::CopyMode;

fn at(path: &str) -> Location {
    Location::new(path).unwrap()
}

fn plan(kind: OpKind) -> OperationPlan {
    OperationPlan {
        kind,
        steps: Vec::new(),
        totals: Totals {
            files: 3,
            folders: 1,
            bytes: 2048,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

fn pending(plan: OperationPlan, target: Option<Location>) -> PendingOp {
    PendingOp {
        id: 1,
        sources: vec![at("/home/a"), at("/home/b.txt")],
        target,
        plan,
        source_tab: AppState::new(at("/home")).active_tab().id(),
    }
}

#[test]
fn a_copy_asks_plainly_and_names_the_target() {
    let props = confirm_props(&pending(plan(OpKind::Copy), Some(at("/dest"))));
    assert_eq!(props.title, "Copy 2 items to /dest?");
    assert_eq!(props.confirm_label, "Copy");
    assert_eq!(props.lines, ["/home/a", "/home/b.txt"]);
    assert_eq!(props.summary, "3 files, 1 folder · 2.0 KB");
    assert!(!props.danger);
    assert!(props.typed.is_none());
}

#[test]
fn each_kind_has_its_own_title() {
    let title = |kind| confirm_props(&pending(plan(kind), Some(at("/dest")))).title;
    assert_eq!(title(OpKind::Move), "Move 2 items to /dest?");
    assert_eq!(title(OpKind::Recycle), "Move 2 items to the Recycle Bin?");
    assert_eq!(title(OpKind::Delete), "Delete 2 items permanently?");
}

#[test]
fn a_permanent_delete_is_dangerous_without_a_typed_word() {
    let props = confirm_props(&pending(plan(OpKind::Delete), None));
    assert!(props.danger);
    assert!(props.typed.is_none());
    assert_eq!(props.confirm_label, "Delete permanently");
}

#[test]
fn a_protected_location_needs_the_typed_word() {
    let mut risky = plan(OpKind::Recycle);
    risky.risks.push(Risk::Protected(at("/home")));
    let props = confirm_props(&pending(risky, None));
    assert!(props.danger);
    assert_eq!(props.typed.as_deref(), Some("DELETE"));
    assert!(props.summary.starts_with("This is a protected location · "));
}

#[test]
fn conflicts_are_counted_in_the_summary() {
    let mut clash = plan(OpKind::Copy);
    clash.conflicts.push(Conflict {
        source: at("/home/b.txt"),
        target: at("/dest/b.txt"),
        resolution: Resolution::Skip,
    });
    let props = confirm_props(&pending(clash, Some(at("/dest"))));
    assert!(
        props
            .summary
            .ends_with(" · 1 item already there will be skipped")
    );
}

fn job(kind: OpKind, status: JobStatus) -> JobView {
    JobView {
        id: 7,
        kind,
        from: Some(at("/home")),
        target: Some(at("/dest")),
        sources: 1,
        progress: Progress {
            bytes_done: 10,
            bytes_total: 20,
            files_done: 1,
            files_total: 2,
            current: Some(at("/home/a")),
            paused: false,
        },
        status,
        cancel: CancelToken::new(),
        source_tab: AppState::new(at("/home")).active_tab().id(),
    }
}

#[test]
fn a_job_card_carries_progress_and_state() {
    let info = job_info(&job(OpKind::Copy, JobStatus::Running), Some(5));
    assert_eq!(info.name(), "Copy 1 item");
    assert_eq!(info.route().as_deref(), Some("home → /dest"));
    assert_eq!(info.current.as_deref(), Some("a"));
    assert_eq!((info.bytes_done, info.bytes_total), (10, 20));
    assert_eq!((info.files_done, info.files_total), (1, 2));
    assert_eq!(info.state, JobRowState::Running);
    assert_eq!(info.speed, Some(5));
}

#[test]
fn every_job_status_maps_to_a_card_state() {
    let state = |kind, status| job_info(&job(kind, status), None).state;
    assert_eq!(state(OpKind::Move, JobStatus::Done), JobRowState::Done);
    assert_eq!(
        state(OpKind::Recycle, JobStatus::Cancelled),
        JobRowState::Cancelled
    );
    assert_eq!(
        state(OpKind::Delete, JobStatus::Failed("denied".to_owned())),
        JobRowState::Failed("denied".into())
    );
    assert_eq!(
        job_info(&job(OpKind::Delete, JobStatus::Done), None).name(),
        "Delete 1 item"
    );
    assert_eq!(
        job_info(&job(OpKind::Recycle, JobStatus::Done), None).name(),
        "Recycle 1 item"
    );
}

#[test]
fn a_copy_into_a_protected_folder_needs_no_typed_word() {
    let mut risky = plan(OpKind::Copy);
    risky.risks.push(Risk::Protected(at("/home")));
    let props = confirm_props(&pending(risky, Some(at("/home"))));
    assert!(!props.danger);
    assert!(props.typed.is_none());
    assert_eq!(props.summary, "3 files, 1 folder · 2.0 KB");
}

#[test]
fn moving_a_protected_source_still_needs_the_typed_word() {
    let mut risky = plan(OpKind::Move);
    risky.risks.push(Risk::Protected(at("/home/a")));
    let props = confirm_props(&pending(risky, Some(at("/dest"))));
    assert!(props.danger);
    assert_eq!(props.typed.as_deref(), Some("DELETE"));
}

#[test]
fn only_a_copy_or_a_move_shows_the_mode() {
    let label = |kind, mode| mode_label(kind, mode).map(|text| text.to_string());
    assert_eq!(label(OpKind::Copy, CopyMode::Auto).as_deref(), Some("Auto"));
    assert_eq!(
        label(OpKind::Move, CopyMode::Standard).as_deref(),
        Some("Standard")
    );
    assert_eq!(label(OpKind::Delete, CopyMode::Auto), None);
    assert_eq!(label(OpKind::Recycle, CopyMode::Standard), None);
}
