use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use navo_core::retention::{MAX_NODES, RetainedDeletePlan, RetainedWritePlan, RetentionPolicy};
use navo_core::{Location, OpKind, OperationPlan, ProviderPath, Resolution, Risk, Step, Totals};
use proptest::prelude::*;

use super::rules::decide;
use super::{
    CantSync, Completion, Deletes, RowRefusal, SafetyNet, SyncAction, SyncBlock, SyncMode,
    SyncPlan, SyncRequest, SyncRules, Verdict, build, recommend,
};
use crate::model::{
    CancelToken, CaseRule, ContentClass, EntryKind, Generation, Reason, RelPath, Row, SideEntry,
    Status,
};
use crate::plan::{NameRules, Side, Sides};
use crate::session::{NameProblem, Refusal};
use crate::text::Direction;

const GENERATION: u64 = 7;
const T0: i64 = 1_700_000_000_000;
const KEPT: Deletes = Deletes::TargetOnly(SafetyNet::Kept);
static RUNNING: LazyLock<CancelToken> = LazyLock::new(CancelToken::new);
const DIFFERENT: Status = Status::Different {
    first_difference: None,
};

fn loc(text: &str) -> Location {
    Location::new(text).expect("test location")
}

fn side(root: &str) -> Side {
    Side {
        root: loc(root),
        case: CaseRule::Sensitive,
        read_only: false,
        names: NameRules::Unix,
    }
}

fn sides() -> Sides {
    Sides {
        left: side("/home/left"),
        right: side("/home/right"),
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

fn folder(path: &str) -> SideEntry {
    entry(path, EntryKind::Folder, 0, None)
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
    }
}

fn left_only(id: u64, item: SideEntry) -> Row {
    row(id, Some(item), None, Status::OnlyLeft)
}

fn right_only(id: u64, item: SideEntry) -> Row {
    row(id, None, Some(item), Status::OnlyRight)
}

fn both(id: u64, left_ms: i64, right_ms: i64) -> Row {
    row(
        id,
        Some(file("a.txt", 10, left_ms)),
        Some(file("a.txt", 12, right_ms)),
        DIFFERENT,
    )
}

const fn rules(mode: SyncMode, deletes: Deletes) -> SyncRules {
    SyncRules { mode, deletes }
}

const TWO_WAY: SyncRules = rules(SyncMode::TwoWay, Deletes::Off);

fn choices(list: &[(u64, SyncAction)]) -> HashMap<u64, SyncAction> {
    list.iter().copied().collect()
}

fn request<'a>(
    rows: &'a [Row],
    sides: &'a Sides,
    rules: SyncRules,
    choices: &'a HashMap<u64, SyncAction>,
) -> SyncRequest<'a> {
    SyncRequest {
        rows,
        generation: Generation::new(GENERATION),
        completion: Completion::Final,
        sides,
        rules,
        choices,
        cancel: &RUNNING,
    }
}

fn run_on(
    rows: &[Row],
    sides: &Sides,
    rules: SyncRules,
    list: &[(u64, SyncAction)],
) -> Result<SyncPlan, SyncBlock> {
    let choices = choices(list);
    build(&request(rows, sides, rules, &choices))
}

fn run(rows: &[Row], rules: SyncRules, list: &[(u64, SyncAction)]) -> Result<SyncPlan, SyncBlock> {
    run_on(rows, &sides(), rules, list)
}

fn labels(plan: &OperationPlan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|step| match step {
            Step::MakeFolder { target } => format!("mk {}", target.as_str()),
            Step::CopyFile { source, target, .. } => {
                format!("cp {} > {}", source.as_str(), target.as_str())
            }
            Step::RemoveFile { location } => format!("rm {}", location.as_str()),
            other => format!("other {other:?}"),
        })
        .collect()
}

fn refusal(row: u64, reason: CantSync) -> RowRefusal {
    RowRefusal { row, reason }
}

const fn totals(files: u64, folders: u64, bytes: u64) -> Totals {
    Totals {
        files,
        folders,
        bytes,
    }
}

#[test]
fn two_way_copies_the_newer_file() {
    let left_newer = both(1, T0 + 10_000, T0);
    let right_newer = both(1, T0, T0 + 10_000);
    assert_eq!(
        recommend(&left_newer, TWO_WAY),
        Verdict::Copy(Direction::LeftToRight)
    );
    assert_eq!(
        recommend(&right_newer, TWO_WAY),
        Verdict::Copy(Direction::RightToLeft)
    );
}

#[test]
fn two_way_times_two_seconds_apart_are_a_conflict() {
    assert_eq!(
        recommend(&both(1, T0 + 2_000, T0), TWO_WAY),
        Verdict::Conflict
    );
    assert_eq!(
        recommend(&both(1, T0 + 2_001, T0), TWO_WAY),
        Verdict::Copy(Direction::LeftToRight)
    );
}

#[test]
fn two_way_without_a_time_is_a_conflict() {
    let untimed = row(
        1,
        Some(file("a.txt", 10, T0 + 60_000)),
        Some(entry("a.txt", EntryKind::File, 12, None)),
        DIFFERENT,
    );
    assert_eq!(recommend(&untimed, TWO_WAY), Verdict::Conflict);
}

#[test]
fn two_way_copies_one_sided_rows_and_never_deletes() {
    let ticked = rules(SyncMode::TwoWay, KEPT);
    assert_eq!(
        recommend(&left_only(1, file("a", 1, T0)), ticked),
        Verdict::Copy(Direction::LeftToRight)
    );
    assert_eq!(
        recommend(&right_only(2, file("b", 1, T0)), ticked),
        Verdict::Copy(Direction::RightToLeft)
    );
}

#[test]
fn not_checked_rows_follow_the_time_rule() {
    let status = Status::NotChecked(Reason::ReadError);
    let same = row(1, Some(file("a", 5, T0)), Some(file("a", 5, T0)), status);
    let newer = row(
        2,
        Some(file("a", 5, T0 + 10_000)),
        Some(file("a", 5, T0)),
        status,
    );
    let mirror = rules(SyncMode::MirrorRight, Deletes::Off);
    assert_eq!(recommend(&same, TWO_WAY), Verdict::Same);
    assert_eq!(recommend(&same, mirror), Verdict::Same);
    assert_eq!(
        recommend(&newer, TWO_WAY),
        Verdict::Copy(Direction::LeftToRight)
    );
}

#[test]
fn mirror_copies_even_when_the_target_is_newer() {
    let mirror = rules(SyncMode::MirrorRight, Deletes::Off);
    assert_eq!(
        recommend(&both(1, T0, T0 + 60_000), mirror),
        Verdict::Copy(Direction::LeftToRight)
    );
}

#[test]
fn mirror_skips_target_only_files_without_the_tick() {
    let right = rules(SyncMode::MirrorRight, Deletes::Off);
    let left = rules(SyncMode::MirrorLeft, Deletes::Off);
    assert_eq!(
        recommend(&right_only(1, file("a", 1, T0)), right),
        Verdict::Skip
    );
    assert_eq!(
        recommend(&left_only(1, file("a", 1, T0)), left),
        Verdict::Skip
    );
}

#[test]
fn mirror_deletes_target_only_files_when_ticked() {
    let right = rules(SyncMode::MirrorRight, KEPT);
    let left = rules(SyncMode::MirrorLeft, KEPT);
    assert_eq!(
        recommend(&right_only(1, file("a", 1, T0)), right),
        Verdict::DeleteTarget
    );
    assert_eq!(
        recommend(&left_only(1, file("a", 1, T0)), left),
        Verdict::DeleteTarget
    );
}

#[test]
fn mirror_never_deletes_a_target_only_folder() {
    let right = rules(SyncMode::MirrorRight, KEPT);
    assert_eq!(
        recommend(&right_only(1, folder("old")), right),
        Verdict::Skip
    );
}

#[test]
fn a_delete_without_the_safety_net_cannot_sync() {
    let right = rules(
        SyncMode::MirrorRight,
        Deletes::TargetOnly(SafetyNet::Missing),
    );
    assert_eq!(
        recommend(&right_only(1, file("a", 1, T0)), right),
        Verdict::CantSync(CantSync::NoSafetyNet)
    );
}

#[test]
fn same_rows_have_nothing_to_do() {
    let modes = [
        SyncMode::TwoWay,
        SyncMode::MirrorRight,
        SyncMode::MirrorLeft,
    ];
    for status in [Status::Same, Status::SameAfterRules, Status::SameSizeTime] {
        let same = row(
            1,
            Some(file("a", 5, T0)),
            Some(file("a", 5, T0 + 60_000)),
            status,
        );
        for mode in modes {
            assert_eq!(
                recommend(&same, rules(mode, KEPT)),
                Verdict::Same,
                "{status:?} {mode:?}"
            );
        }
    }
}

#[test]
fn checking_rows_wait() {
    let checking = row(
        1,
        Some(file("a", 5, T0)),
        Some(file("a", 5, T0)),
        Status::Checking,
    );
    assert_eq!(recommend(&checking, TWO_WAY), Verdict::Waiting);
}

#[test]
fn rows_the_compare_refuses_cannot_sync() {
    let cases = [
        (
            row(
                1,
                Some(file("x", 1, T0)),
                Some(folder("x")),
                Status::TypeConflict,
            ),
            Refusal::TypeConflict,
        ),
        (
            row(
                2,
                Some(file("A", 1, T0)),
                Some(file("a", 1, T0)),
                Status::CaseConflict,
            ),
            Refusal::CaseConflict,
        ),
        (
            row(
                3,
                Some(folder("d")),
                Some(folder("d")),
                Status::Unavailable(Reason::AccessDenied),
            ),
            Refusal::Unavailable(Reason::AccessDenied),
        ),
        (
            left_only(4, entry("l", EntryKind::Link, 0, Some(T0))),
            Refusal::Link,
        ),
        (
            left_only(5, entry("s", EntryKind::Submodule, 0, Some(T0))),
            Refusal::Submodule,
        ),
    ];
    for (refused, reason) in cases {
        assert_eq!(
            recommend(&refused, TWO_WAY),
            Verdict::CantSync(CantSync::Row(reason)),
            "row {}",
            refused.id
        );
    }
}

#[test]
fn ignored_rows_never_sync() {
    let mut ignored = file("a", 1, T0);
    ignored.ignored = true;
    let ignored = left_only(1, ignored);
    let expected = Verdict::CantSync(CantSync::Ignored);
    assert_eq!(recommend(&ignored, TWO_WAY), expected);
    assert_eq!(
        decide(&ignored, TWO_WAY, Some(SyncAction::CopyRight)),
        expected
    );
}

#[test]
fn a_choice_overrides_the_recommendation() {
    let conflict = both(1, T0 + 1_000, T0);
    let same = row(
        2,
        Some(file("a", 5, T0)),
        Some(file("a", 5, T0)),
        Status::Same,
    );
    assert_eq!(
        decide(&conflict, TWO_WAY, Some(SyncAction::CopyLeft)),
        Verdict::Copy(Direction::RightToLeft)
    );
    assert_eq!(
        decide(&same, TWO_WAY, Some(SyncAction::CopyRight)),
        Verdict::Copy(Direction::LeftToRight)
    );
}

#[test]
fn a_choice_that_does_not_fit_cannot_sync() {
    let ticked = rules(SyncMode::MirrorRight, KEPT);
    let unticked = rules(SyncMode::MirrorRight, Deletes::Off);
    let only_left = left_only(1, file("a", 1, T0));
    let only_right = right_only(2, file("b", 1, T0));
    let different = both(3, T0 + 10_000, T0);
    let cases = [
        decide(&only_left, TWO_WAY, Some(SyncAction::CopyLeft)),
        decide(
            &only_right,
            rules(SyncMode::TwoWay, KEPT),
            Some(SyncAction::DeleteTarget),
        ),
        decide(&different, ticked, Some(SyncAction::DeleteTarget)),
        decide(&only_right, unticked, Some(SyncAction::DeleteTarget)),
    ];
    for verdict in cases {
        assert_eq!(verdict, Verdict::CantSync(CantSync::ChoiceNotAllowed));
    }
}

#[test]
fn skip_wins_over_every_state() {
    let checking = row(
        1,
        Some(file("a", 5, T0)),
        Some(file("a", 5, T0)),
        Status::Checking,
    );
    let typed = row(
        2,
        Some(file("x", 1, T0)),
        Some(folder("x")),
        Status::TypeConflict,
    );
    assert_eq!(
        decide(&checking, TWO_WAY, Some(SyncAction::Skip)),
        Verdict::Skip
    );
    assert_eq!(
        decide(&typed, TWO_WAY, Some(SyncAction::Skip)),
        Verdict::Skip
    );
}

#[test]
fn copies_run_both_ways_in_one_copy_part() {
    let rows = [
        both(1, T0 + 10_000, T0),
        right_only(2, file("b.txt", 4, T0)),
    ];
    let plan = run(&rows, TWO_WAY, &[]).expect("plan");
    let copy = plan.copy.expect("copy part");
    assert_eq!(copy.kind, OpKind::Copy);
    assert_eq!(
        labels(&copy),
        [
            "cp /home/left/a.txt > /home/right/a.txt",
            "cp /home/right/b.txt > /home/left/b.txt",
        ]
    );
    assert_eq!(copy.conflicts.len(), 1);
    assert_eq!(copy.conflicts[0].target, loc("/home/right/a.txt"));
    assert_eq!(copy.conflicts[0].resolution, Resolution::Overwrite);
    assert_eq!(copy.risks, [Risk::Overwrite(loc("/home/right/a.txt"))]);
    assert_eq!(copy.totals, totals(2, 0, 14));
    assert_eq!(plan.summary.to_right, totals(1, 0, 10));
    assert_eq!(plan.summary.to_left, totals(1, 0, 4));
    assert!(plan.delete.is_none());
}

#[test]
fn a_new_folder_is_made_once_before_its_files() {
    let rows = [
        left_only(1, folder("new")),
        left_only(2, file("new/x", 1, T0)),
        left_only(3, file("new/y", 2, T0)),
        left_only(4, file("new/sub/z", 3, T0)),
    ];
    let copy = run(&rows, TWO_WAY, &[]).expect("plan").copy.expect("copy");
    assert_eq!(
        labels(&copy),
        [
            "mk /home/right/new",
            "mk /home/right/new/sub",
            "cp /home/left/new/sub/z > /home/right/new/sub/z",
            "cp /home/left/new/x > /home/right/new/x",
            "cp /home/left/new/y > /home/right/new/y",
        ]
    );
    assert_eq!(copy.totals, totals(3, 2, 6));
}

#[test]
fn existing_target_folders_are_not_made() {
    let rows = [
        row(1, Some(folder("docs")), Some(folder("docs")), Status::Same),
        left_only(2, file("docs/a.txt", 3, T0)),
    ];
    let copy = run(&rows, TWO_WAY, &[]).expect("plan").copy.expect("copy");
    assert_eq!(
        labels(&copy),
        ["cp /home/left/docs/a.txt > /home/right/docs/a.txt"]
    );
    assert_eq!(copy.totals, totals(1, 0, 3));
}

#[test]
fn an_empty_one_sided_folder_is_made() {
    let plan = run(&[left_only(1, folder("empty"))], TWO_WAY, &[]).expect("plan");
    let copy = plan.copy.expect("copy");
    assert_eq!(labels(&copy), ["mk /home/right/empty"]);
    assert_eq!(plan.summary.to_right, totals(0, 1, 0));
}

fn delete_rows() -> Vec<Row> {
    vec![
        right_only(1, file("old.txt", 5, T0)),
        right_only(2, file("dir/old2.txt", 7, T0)),
        left_only(3, file("new.txt", 2, T0)),
    ]
}

#[test]
fn deletes_go_to_their_own_delete_part() {
    let plan = run(&delete_rows(), rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    let delete = plan.delete.expect("delete part");
    assert_eq!(delete.kind, OpKind::Delete);
    assert_eq!(
        labels(&delete),
        ["rm /home/right/dir/old2.txt", "rm /home/right/old.txt"]
    );
    assert_eq!(delete.totals, totals(2, 0, 12));
    assert!(delete.conflicts.is_empty() && delete.risks.is_empty());
    assert_eq!(plan.summary.deleted, totals(2, 0, 12));
    let copy = plan.copy.expect("copy part");
    assert!(
        copy.steps
            .iter()
            .all(|step| matches!(step, Step::MakeFolder { .. } | Step::CopyFile { .. }))
    );
}

#[test]
fn the_parts_pass_the_safety_net_checks() {
    let mut rows = delete_rows();
    rows.push(left_only(4, folder("fresh")));
    rows.push(left_only(5, file("fresh/f", 1, T0)));
    rows.push(both(6, T0 + 10_000, T0));
    let plan = run(&rows, rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    let policy = RetentionPolicy::new(3, 1 << 40, 0).expect("policy");
    let copy = plan.copy.expect("copy");
    let delete = plan.delete.expect("delete");
    RetainedWritePlan::new(copy, policy).expect("retained copy");
    RetainedDeletePlan::new(delete, policy).expect("retained delete");
}

#[test]
fn rows_below_a_blocked_folder_cannot_sync() {
    let rows = [
        row(
            1,
            Some(folder("x")),
            Some(file("x", 1, T0)),
            Status::TypeConflict,
        ),
        left_only(2, file("x/a.txt", 1, T0)),
    ];
    let plan = run(&rows, TWO_WAY, &[]).expect("plan");
    assert!(plan.copy.is_none());
    assert_eq!(
        plan.summary.cant_sync,
        [
            refusal(1, CantSync::Row(Refusal::TypeConflict)),
            refusal(2, CantSync::BelowBlocked),
        ]
    );
}

#[test]
fn an_unreadable_folder_never_turns_into_deletes() {
    let rows = [
        row(
            1,
            Some(folder("src")),
            Some(folder("src")),
            Status::Unavailable(Reason::AccessDenied),
        ),
        right_only(2, file("src/f.txt", 1, T0)),
    ];
    let plan = run(&rows, rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    assert!(plan.delete.is_none());
    assert!(
        plan.summary
            .cant_sync
            .contains(&refusal(2, CantSync::BelowBlocked))
    );
}

#[test]
fn a_skipped_folder_skips_everything_below() {
    let rows = [
        left_only(1, folder("keep")),
        left_only(2, file("keep/a", 1, T0)),
        left_only(3, file("keep/deep/b", 1, T0)),
    ];
    let plan = run(&rows, TWO_WAY, &[(1, SyncAction::Skip)]).expect("plan");
    assert!(plan.copy.is_none());
    assert_eq!(plan.summary.skipped, 3);
}

fn read_only_right() -> Sides {
    let mut sides = sides();
    sides.right.read_only = true;
    sides
}

#[test]
fn a_copy_into_a_read_only_side_cannot_sync() {
    let rows = [
        left_only(1, file("l.txt", 1, T0)),
        right_only(2, file("r.txt", 1, T0)),
    ];
    let plan = run_on(&rows, &read_only_right(), TWO_WAY, &[]).expect("plan");
    assert_eq!(
        plan.summary.cant_sync,
        [refusal(1, CantSync::Row(Refusal::ReadOnlyTarget))]
    );
    let copy = plan.copy.expect("copy");
    assert_eq!(labels(&copy), ["cp /home/right/r.txt > /home/left/r.txt"]);
}

#[test]
fn a_delete_on_a_read_only_side_cannot_sync() {
    let rows = [right_only(1, file("old.txt", 1, T0))];
    let ticked = rules(SyncMode::MirrorRight, KEPT);
    let plan = run_on(&rows, &read_only_right(), ticked, &[]).expect("plan");
    assert!(plan.delete.is_none());
    assert_eq!(
        plan.summary.cant_sync,
        [refusal(1, CantSync::Row(Refusal::ReadOnlyTarget))]
    );
}

#[test]
fn two_writes_to_one_path_on_a_case_insensitive_side_cannot_sync() {
    let mut sides = sides();
    sides.right.case = CaseRule::Insensitive;
    let rows = [
        left_only(1, file("Read.me", 1, T0)),
        right_only(2, file("read.me", 1, T0)),
    ];
    let plan = run_on(&rows, &sides, rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    assert!(plan.copy.is_none() && plan.delete.is_none());
    let mut refused = plan.summary.cant_sync;
    refused.sort_by_key(|refusal| refusal.row);
    assert_eq!(
        refused,
        [
            refusal(1, CantSync::PathCollision),
            refusal(2, CantSync::PathCollision),
        ]
    );
}

#[test]
fn windows_names_cannot_sync() {
    let mut sides = sides();
    sides.right.names = NameRules::Windows;
    let plan = run_on(
        &[left_only(1, file("aux.txt", 1, T0))],
        &sides,
        TWO_WAY,
        &[],
    )
    .expect("plan");
    assert!(plan.copy.is_none());
    assert_eq!(
        plan.summary.cant_sync,
        [refusal(
            1,
            CantSync::Row(Refusal::NameUnwritable(NameProblem::Reserved))
        )]
    );
}

fn unresolved_rows() -> [Row; 2] {
    [
        both(1, T0 + 1_000, T0),
        row(
            2,
            Some(file("b", 1, T0)),
            Some(file("b", 1, T0)),
            Status::Checking,
        ),
    ]
}

#[test]
fn open_conflicts_and_checking_rows_block_the_plan() {
    assert_eq!(
        run(&unresolved_rows(), TWO_WAY, &[]),
        Err(SyncBlock::Unresolved {
            conflicts: vec![1],
            checking: vec![2],
        })
    );
}

#[test]
fn a_skipped_conflict_lets_the_plan_run() {
    let skips = [(1, SyncAction::Skip), (2, SyncAction::Skip)];
    let plan = run(&unresolved_rows(), TWO_WAY, &skips).expect("plan");
    assert!(plan.copy.is_none());
    assert_eq!(plan.summary.skipped, 2);
}

#[test]
fn only_a_final_compare_plans() {
    let rows = [left_only(1, file("a", 1, T0))];
    let sides = sides();
    let choices = HashMap::new();
    for completion in [
        Completion::Walking,
        Completion::StoppedAtLimit,
        Completion::Cancelled,
        Completion::Failed(Reason::WorkerPanicked),
    ] {
        let mut asked = request(&rows, &sides, TWO_WAY, &choices);
        asked.completion = completion;
        assert_eq!(build(&asked), Err(SyncBlock::NotFinal(completion)));
    }
}

#[test]
fn a_row_from_another_generation_refuses_the_plan() {
    let mut stale = left_only(2, file("b", 1, T0));
    stale.generation = Generation::new(GENERATION + 1);
    let rows = [left_only(1, file("a", 1, T0)), stale];
    assert_eq!(run(&rows, TWO_WAY, &[]), Err(SyncBlock::StaleGeneration));
}

#[test]
fn a_duplicate_row_id_refuses_the_plan() {
    let rows = [
        left_only(4, file("a", 1, T0)),
        left_only(4, file("b", 1, T0)),
    ];
    assert_eq!(run(&rows, TWO_WAY, &[]), Err(SyncBlock::DuplicateRow(4)));
}

#[test]
fn a_choice_for_a_missing_row_refuses_the_plan() {
    let rows = [left_only(1, file("a", 1, T0))];
    assert_eq!(
        run(&rows, TWO_WAY, &[(99, SyncAction::CopyRight)]),
        Err(SyncBlock::UnknownChoice(99))
    );
}

fn many_files(count: usize) -> Vec<Row> {
    (0..count)
        .map(|index| {
            let id = u64::try_from(index).expect("id");
            left_only(id, file(&format!("f{index}"), 1, T0))
        })
        .collect()
}

#[test]
fn a_part_at_the_node_limit_plans() {
    let plan = run(&many_files(MAX_NODES), TWO_WAY, &[]).expect("plan");
    assert_eq!(plan.copy.expect("copy").steps.len(), MAX_NODES);
}

#[test]
fn a_part_over_the_node_limit_refuses_the_plan() {
    assert_eq!(
        run(&many_files(MAX_NODES + 1), TWO_WAY, &[]),
        Err(SyncBlock::TooManySteps {
            steps: MAX_NODES + 1
        })
    );
}

#[test]
fn mirror_counts_the_newer_files_it_replaces() {
    let mut older = both(2, T0 + 10_000, T0);
    for item in [&mut older.left, &mut older.right].into_iter().flatten() {
        *item = Arc::new(file("b.txt", item.size, item.modified.unwrap_or(0)));
    }
    let rows = [both(1, T0, T0 + 10_000), older];
    let plan = run(&rows, rules(SyncMode::MirrorRight, Deletes::Off), &[]).expect("plan");
    assert_eq!(plan.summary.replaces_newer, 1);
    assert_eq!(plan.summary.to_right.files, 2);
}

#[test]
fn nothing_to_sync_gives_no_parts() {
    let rows = [
        row(
            1,
            Some(file("a", 1, T0)),
            Some(file("a", 1, T0)),
            Status::Same,
        ),
        row(2, Some(folder("d")), Some(folder("d")), Status::Same),
    ];
    let plan = run(&rows, rules(SyncMode::MirrorRight, KEPT), &[]).expect("plan");
    assert!(plan.copy.is_none() && plan.delete.is_none());
    assert_eq!(plan.summary.same, 2);
}

#[test]
fn the_parts_do_not_depend_on_row_order() {
    let mut rows = delete_rows();
    rows.push(left_only(4, folder("fresh")));
    rows.push(left_only(5, file("fresh/f", 1, T0)));
    rows.push(both(6, T0 + 10_000, T0));
    let ticked = rules(SyncMode::MirrorRight, KEPT);
    let forward = run(&rows, ticked, &[]).expect("forward");
    rows.reverse();
    let backward = run(&rows, ticked, &[]).expect("backward");
    assert!(forward.copy.is_some() && forward.delete.is_some());
    assert_eq!(forward.copy, backward.copy);
    assert_eq!(forward.delete, backward.delete);
}

fn arbitrary_row(index: usize, shape: u8, is_folder: bool) -> Row {
    let id = u64::try_from(index).expect("id");
    let name = format!("f{index}");
    let make = |modified: i64| {
        if is_folder {
            folder(&name)
        } else {
            file(&name, 3, modified)
        }
    };
    match shape {
        0 => left_only(id, make(T0)),
        1 => right_only(id, make(T0)),
        2 => row(id, Some(make(T0 + 10_000)), Some(make(T0)), DIFFERENT),
        _ => row(id, Some(make(T0)), Some(make(T0)), Status::Same),
    }
}

fn target_only_file(row: &Row, master: Direction) -> Option<String> {
    let (source, target, root) = match master {
        Direction::LeftToRight => (&row.left, &row.right, "/home/right"),
        Direction::RightToLeft => (&row.right, &row.left, "/home/left"),
    };
    let target = target.as_deref().filter(|_| source.is_none())?;
    (target.kind == EntryKind::File).then(|| format!("{root}/{}", target.rel.as_path().to_text()))
}

fn check_deletes(rows: &[Row], rules: SyncRules, plan: &SyncPlan) -> Result<(), TestCaseError> {
    let master = match rules.mode {
        SyncMode::TwoWay => None,
        SyncMode::MirrorRight => Some(Direction::LeftToRight),
        SyncMode::MirrorLeft => Some(Direction::RightToLeft),
    };
    let Some(master) = master.filter(|_| rules.deletes == KEPT) else {
        prop_assert!(plan.delete.is_none());
        return Ok(());
    };
    let allowed: Vec<String> = rows
        .iter()
        .filter_map(|row| target_only_file(row, master))
        .collect();
    for step in plan.delete.iter().flat_map(|part| &part.steps) {
        let Step::RemoveFile { location } = step else {
            return Err(TestCaseError::fail(format!("not a file delete: {step:?}")));
        };
        prop_assert!(allowed.iter().any(|path| path == location.as_str()));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn deletes_only_hit_target_only_files(
        shapes in prop::collection::vec((0_u8..4, any::<bool>(), 0_u8..3), 0..40),
        mode in 0_u8..3,
        ticked in any::<bool>(),
    ) {
        let rows: Vec<Row> = shapes
            .iter()
            .enumerate()
            .map(|(index, &(shape, is_folder, _))| arbitrary_row(index, shape, is_folder))
            .collect();
        let list: Vec<(u64, SyncAction)> = shapes
            .iter()
            .zip(&rows)
            .filter_map(|(&(_, _, choice), row)| match choice {
                1 => Some((row.id, SyncAction::Skip)),
                2 => Some((row.id, SyncAction::DeleteTarget)),
                _ => None,
            })
            .collect();
        let mode = [SyncMode::TwoWay, SyncMode::MirrorRight, SyncMode::MirrorLeft][usize::from(mode)];
        let rules = rules(mode, if ticked { KEPT } else { Deletes::Off });
        if let Ok(plan) = run(&rows, rules, &list) {
            check_deletes(&rows, rules, &plan)?;
            for step in plan.copy.iter().flat_map(|part| &part.steps) {
                let kept = matches!(step, Step::MakeFolder { .. } | Step::CopyFile { .. });
                prop_assert!(kept, "copy part step {:?}", step);
            }
        }
    }
}

mod fixes;
