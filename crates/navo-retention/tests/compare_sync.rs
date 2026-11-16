//! The compare sync plan's parts pass the retention safety-net checks (moved from the engine, I2).

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test fixture")]

use std::sync::Arc;

use navo_compare::model::{
    CancelToken, CaseRule, ContentClass, EntryKind, Generation, RelPath, Row, SideEntry, Status,
};
use navo_compare::plan::sync::{
    Completion, Deletes, SafetyNet, SyncAction, SyncBlock, SyncMode, SyncPlan, SyncRequest,
    SyncRules, build,
};
use navo_compare::plan::{NameRules, Side, Sides};
use navo_core::retention::{RetainedDeletePlan, RetainedWritePlan, RetentionPolicy};
use navo_core::{Location, ProviderPath};

const GENERATION: u64 = 7;
const T0: i64 = 1_700_000_000_000;
const KEPT: Deletes = Deletes::TargetOnly(SafetyNet::Kept);
fn side(root: &str) -> Side {
    Side {
        root: Location::new(root).expect("test location"),
        case: CaseRule::Sensitive,
        read_only: false,
        names: NameRules::Unix,
    }
}

fn entry(path: &str, kind: EntryKind, size: u64, modified: Option<i64>) -> SideEntry {
    SideEntry::new(
        RelPath::new(ProviderPath::from_text(path)),
        kind,
        size,
        modified,
    )
}

fn file(path: &str, size: u64, modified_ms: i64) -> SideEntry {
    entry(path, EntryKind::File, size, Some(modified_ms))
}

fn row(id: u64, left: Option<SideEntry>, right: Option<SideEntry>, status: Status) -> Row {
    Row {
        id,
        left: left.map(Arc::new),
        right: right.map(Arc::new),
        raw_status: status,
        display_status: status,
        generation: Generation::new(GENERATION),
        content: ContentClass::Unknown,
        contains_differences: false,
        differences_inside: 0,
    }
}

fn left_only(id: u64, item: SideEntry) -> Row {
    row(id, Some(item), None, Status::OnlyLeft)
}

fn right_only(id: u64, item: SideEntry) -> Row {
    row(id, None, Some(item), Status::OnlyRight)
}

fn both(id: u64, left_ms: i64, right_ms: i64) -> Row {
    let status = Status::Different {
        first_difference: None,
    };
    let left = file("a.txt", 10, left_ms);
    row(id, Some(left), Some(file("a.txt", 12, right_ms)), status)
}

const fn rules(mode: SyncMode, deletes: Deletes) -> SyncRules {
    SyncRules { mode, deletes }
}

fn run(rows: &[Row], rules: SyncRules, list: &[(u64, SyncAction)]) -> Result<SyncPlan, SyncBlock> {
    let choices = list.iter().copied().collect();
    let sides = Sides {
        left: side("/home/left"),
        right: side("/home/right"),
    };
    build(&SyncRequest {
        rows,
        generation: Generation::new(GENERATION),
        completion: Completion::Final,
        sides: &sides,
        rules,
        choices: &choices,
        cancel: &CancelToken::new(),
    })
}

fn delete_rows() -> Vec<Row> {
    vec![
        right_only(1, file("old.txt", 5, T0)),
        right_only(2, file("dir/old2.txt", 7, T0)),
        left_only(3, file("new.txt", 2, T0)),
    ]
}

#[test]
fn the_parts_pass_the_safety_net_checks() {
    let mut rows = delete_rows();
    rows.push(left_only(4, entry("fresh", EntryKind::Folder, 0, None)));
    rows.push(left_only(5, file("fresh/f", 1, T0)));
    rows.push(both(6, T0 + 10_000, T0));
    let plan = run(&rows, rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    let policy = RetentionPolicy::new(3, 1 << 40, 0).expect("policy");
    let copy = plan.copy.expect("copy");
    let delete = plan.delete.expect("delete");
    RetainedWritePlan::new(copy, policy).expect("retained copy");
    RetainedDeletePlan::new(delete, policy).expect("retained delete");
}
