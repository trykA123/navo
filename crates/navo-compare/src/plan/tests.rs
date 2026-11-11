use std::sync::Arc;

use navo_core::ProviderId;
use navo_core::{Conflict, OpKind, OperationPlan, Resolution, Risk, Step};

use super::{NameRules, Side, Sides, plan};
use crate::model::{
    CaseRule, ContentClass, EntryKind, Generation, RelPath, Row, SideEntry, Status,
};
use crate::pair::pair;
use crate::session::{NameProblem, Refusal};
use crate::text::Direction;

const GENERATION: u64 = 7;

fn generation() -> Generation {
    Generation::new(GENERATION)
}

fn loc(text: &str) -> navo_core::Location {
    navo_core::Location::new(text).expect("test location")
}

fn side(root: &str) -> Side {
    Side {
        root: loc(root),
        case: CaseRule::Sensitive,
        read_only: false,
        names: NameRules::Unix,
    }
}

fn windows_side(root: &str) -> Side {
    Side {
        names: NameRules::Windows,
        ..side(root)
    }
}

fn insensitive_side(root: &str) -> Side {
    Side {
        case: CaseRule::Insensitive,
        ..side(root)
    }
}

fn sides() -> Sides {
    Sides {
        left: side("left"),
        right: side("right"),
    }
}

fn entry(path: &str, kind: EntryKind, size: u64) -> SideEntry {
    SideEntry::new(
        RelPath::new(navo_core::ProviderPath::from_text(path)),
        kind,
        size,
        None,
    )
}

fn file(path: &str, size: u64) -> SideEntry {
    entry(path, EntryKind::File, size)
}

fn folder(path: &str) -> SideEntry {
    entry(path, EntryKind::Folder, 0)
}

fn row(left: Option<SideEntry>, right: Option<SideEntry>, status: Status) -> Row {
    Row {
        id: 1,
        left: left.map(Arc::new),
        right: right.map(Arc::new),
        raw_status: status,
        display_status: status,
        generation: generation(),
        content: ContentClass::Unknown,
        contains_differences: false,
        differences_inside: 0,
    }
}

fn copy_plan(row: Row, direction: Direction, sides: &Sides) -> Result<OperationPlan, Refusal> {
    plan(&[row], direction, generation(), sides)
}

fn copy_step(plan: &OperationPlan) -> (&navo_core::Location, &navo_core::Location, u64) {
    plan.steps
        .iter()
        .find_map(|step| match step {
            Step::CopyFile {
                source,
                target,
                size,
            } => Some((source, target, *size)),
            _ => None,
        })
        .expect("a copy step")
}

fn folder_targets(plan: &OperationPlan) -> Vec<&str> {
    plan.steps
        .iter()
        .filter_map(|step| match step {
            Step::MakeFolder { target } => Some(target.as_str()),
            _ => None,
        })
        .collect()
}

fn assert_only_copy_steps(plan: &OperationPlan) {
    assert!(
        plan.steps
            .iter()
            .all(|step| matches!(step, Step::CopyFile { .. } | Step::MakeFolder { .. })),
        "a copy plan must never delete destination-only files"
    );
}

#[test]
fn a_left_only_file_copies_to_the_right_side() {
    let plan = copy_plan(
        row(Some(file("one.txt", 12)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a left-only file copies right");
    assert_only_copy_steps(&plan);
    assert_eq!(plan.kind, OpKind::Copy);
    let (source, target, size) = copy_step(&plan);
    assert_eq!(source.as_str(), "left/one.txt");
    assert_eq!(target.as_str(), "right/one.txt");
    assert_eq!(size, 12);
}

#[test]
fn a_right_only_file_copies_to_the_left_side() {
    let plan = copy_plan(
        row(None, Some(file("two.txt", 34)), Status::OnlyRight),
        Direction::RightToLeft,
        &sides(),
    )
    .expect("a right-only file copies left");
    assert_only_copy_steps(&plan);
    let (source, target, size) = copy_step(&plan);
    assert_eq!(source.as_str(), "right/two.txt");
    assert_eq!(target.as_str(), "left/two.txt");
    assert_eq!(size, 34);
}

#[test]
fn a_left_only_row_has_no_source_to_copy_right_to_left() {
    let refusal = copy_plan(
        row(Some(file("one.txt", 12)), None, Status::OnlyLeft),
        Direction::RightToLeft,
        &sides(),
    )
    .expect_err("a left-only row has no right source");
    assert_eq!(refusal, Refusal::NoSource);
}

#[test]
fn a_right_only_row_has_no_source_to_copy_left_to_right() {
    let refusal = copy_plan(
        row(None, Some(file("two.txt", 34)), Status::OnlyRight),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("a right-only row has no left source");
    assert_eq!(refusal, Refusal::NoSource);
}

#[test]
fn a_changed_file_copies_the_source_side_size() {
    let plan = copy_plan(
        row(
            Some(file("report.txt", 40)),
            Some(file("report.txt", 5)),
            Status::Different {
                first_difference: Some(3),
            },
        ),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a changed file copies right");
    assert_only_copy_steps(&plan);
    let (source, target, size) = copy_step(&plan);
    assert_eq!(source.as_str(), "left/report.txt");
    assert_eq!(target.as_str(), "right/report.txt");
    assert_eq!(size, 40);
    assert_eq!(
        plan.totals,
        navo_core::Totals {
            files: 1,
            folders: 0,
            bytes: 40,
        }
    );
}

#[test]
fn a_changed_file_plans_one_overwrite_conflict() {
    let plan = copy_plan(
        row(
            Some(file("report.txt", 40)),
            Some(file("report.txt", 5)),
            Status::Different {
                first_difference: Some(3),
            },
        ),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a changed file copies right");
    assert_eq!(
        plan.conflicts,
        vec![Conflict {
            source: loc("left/report.txt"),
            target: loc("right/report.txt"),
            resolution: Resolution::Overwrite,
        }]
    );
    assert_eq!(plan.risks, vec![Risk::Overwrite(loc("right/report.txt"))]);
}

#[test]
fn a_one_sided_copy_plans_no_overwrite() {
    let plan = copy_plan(
        row(Some(file("one.txt", 12)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a left-only file copies right");
    assert!(plan.conflicts.is_empty());
    assert!(plan.risks.is_empty());
}

#[test]
fn a_missing_folder_chain_is_planned_before_the_copy() {
    let plan = copy_plan(
        row(Some(file("a/b/c.txt", 9)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a nested file copies right");
    assert_only_copy_steps(&plan);
    assert_eq!(folder_targets(&plan), ["right/a", "right/a/b"]);
    let (source, target, _) = copy_step(&plan);
    assert_eq!(source.as_str(), "left/a/b/c.txt");
    assert_eq!(target.as_str(), "right/a/b/c.txt");
}

#[test]
fn a_target_entry_means_its_parent_folders_exist() {
    let plan = copy_plan(
        row(
            Some(file("a/b.txt", 9)),
            Some(file("a/b.txt", 4)),
            Status::Different {
                first_difference: None,
            },
        ),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a nested changed file copies right");
    assert!(folder_targets(&plan).is_empty());
}

#[test]
fn a_case_insensitive_pair_targets_the_existing_spelling() {
    let sides = Sides {
        left: side("left"),
        right: insensitive_side("right"),
    };
    let mut rows = pair(
        vec![file("Readme.md", 4)],
        vec![file("readme.md", 4)],
        CaseRule::Insensitive,
    );
    assert_eq!(rows.len(), 1);
    let mut row = rows.pop().expect("the paired row");
    row.raw_status = Status::Different {
        first_difference: None,
    };
    row.display_status = row.raw_status;
    row.generation = generation();
    let plan = copy_plan(row, Direction::LeftToRight, &sides).expect("the pair copies right");
    assert_only_copy_steps(&plan);
    let (source, target, _) = copy_step(&plan);
    assert_eq!(source.as_str(), "left/Readme.md");
    assert_eq!(target.as_str(), "right/readme.md");
    assert_eq!(plan.conflicts[0].target.as_str(), "right/readme.md");
    assert!(folder_targets(&plan).is_empty());
}

#[test]
fn a_folder_row_with_a_missing_parent_chain_is_refused() {
    let refusal = copy_plan(
        row(Some(folder("a/b")), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("a folder row is not one file");
    assert_eq!(refusal, Refusal::NotAFile);
}

#[test]
fn an_empty_selection_is_refused() {
    let refusal = plan(&[], Direction::LeftToRight, generation(), &sides())
        .expect_err("a copy needs one row");
    assert_eq!(refusal, Refusal::NoRow);
}

#[test]
fn more_than_one_row_is_refused() {
    let rows = [
        row(Some(file("one.txt", 1)), None, Status::OnlyLeft),
        row(Some(file("two.txt", 2)), None, Status::OnlyLeft),
    ];
    let refusal = plan(&rows, Direction::LeftToRight, generation(), &sides())
        .expect_err("one copy is one row");
    assert_eq!(refusal, Refusal::MultipleRows);
}

#[test]
fn a_stale_generation_is_refused() {
    let mut row = row(Some(file("one.txt", 1)), None, Status::OnlyLeft);
    row.generation = Generation::new(GENERATION - 1);
    let refusal = copy_plan(row, Direction::LeftToRight, &sides())
        .expect_err("a plan on an older generation is refused");
    assert_eq!(refusal, Refusal::StaleGeneration);
}

#[test]
fn a_checking_row_is_refused() {
    let refusal = copy_plan(
        row(Some(file("one.txt", 1)), None, Status::Checking),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("a row still being checked is refused");
    assert_eq!(refusal, Refusal::Checking);
}

#[test]
fn a_link_row_is_refused() {
    let refusal = copy_plan(
        row(
            Some(entry("one.link", EntryKind::Link, 0)),
            None,
            Status::OnlyLeft,
        ),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("a link is not copied");
    assert_eq!(refusal, Refusal::Link);
}

#[test]
fn a_submodule_row_is_refused() {
    let refusal = copy_plan(
        row(
            None,
            Some(entry("sub", EntryKind::Submodule, 0)),
            Status::OnlyRight,
        ),
        Direction::RightToLeft,
        &sides(),
    )
    .expect_err("a submodule is not copied");
    assert_eq!(refusal, Refusal::Submodule);
}

#[test]
fn an_unavailable_row_is_refused() {
    let reason = crate::model::Reason::NotFound;
    let refusal = copy_plan(
        row(Some(file("one.txt", 1)), None, Status::Unavailable(reason)),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("an unavailable row is refused");
    assert_eq!(refusal, Refusal::Unavailable(reason));
}

#[test]
fn a_read_only_target_is_refused() {
    let sides = Sides {
        left: side("left"),
        right: Side {
            read_only: true,
            ..side("right")
        },
    };
    let refusal = copy_plan(
        row(Some(file("one.txt", 1)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides,
    )
    .expect_err("a read-only target is refused");
    assert_eq!(refusal, Refusal::ReadOnlyTarget);
}

#[test]
fn a_type_conflict_is_refused() {
    let refusal = copy_plan(
        row(
            Some(file("one", 1)),
            Some(folder("one")),
            Status::TypeConflict,
        ),
        Direction::LeftToRight,
        &sides(),
    )
    .expect_err("a file against a folder is refused");
    assert_eq!(refusal, Refusal::TypeConflict);
}

#[test]
fn a_case_only_twin_on_an_insensitive_target_is_refused() {
    let sides = Sides {
        left: side("left"),
        right: insensitive_side("right"),
    };
    let rows = pair(
        vec![file("Readme.md", 4)],
        vec![file("README.md", 4), file("readme.md", 4)],
        CaseRule::Insensitive,
    );
    assert!(
        rows.iter()
            .all(|row| row.raw_status == Status::CaseConflict)
    );
    let mut row = rows
        .into_iter()
        .find(|row| row.left.is_some())
        .expect("the paired row");
    row.generation = generation();
    let refusal = copy_plan(row, Direction::LeftToRight, &sides)
        .expect_err("a case-only twin is refused on a case-insensitive target");
    assert_eq!(refusal, Refusal::CaseConflict);
}

#[test]
fn a_backslash_is_part_of_one_name() {
    let plan = copy_plan(
        row(Some(file("dir/a\\b.txt", 3)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a literal backslash stays inside a name");
    assert_only_copy_steps(&plan);
    let (source, target, _) = copy_step(&plan);
    assert_eq!(source.as_str(), "left/dir/a\\b.txt");
    assert_eq!(target.as_str(), "right/dir/a\\b.txt");
}

#[test]
fn a_backslash_name_cannot_be_written_on_windows() {
    let sides = Sides {
        left: side("left"),
        right: windows_side("right"),
    };
    let refusal = copy_plan(
        row(Some(file("a\\b.txt", 3)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides,
    )
    .expect_err("a backslash is illegal in a Windows name");
    assert_eq!(
        refusal,
        Refusal::NameUnwritable(NameProblem::IllegalCharacter)
    );
}

#[test]
fn a_reserved_windows_name_is_refused() {
    let sides = Sides {
        left: side("left"),
        right: windows_side("right"),
    };
    let refusal = copy_plan(
        row(Some(file("CON.txt", 3)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides,
    )
    .expect_err("a reserved DOS name cannot be written");
    assert_eq!(refusal, Refusal::NameUnwritable(NameProblem::Reserved));
}

#[test]
fn a_git_source_keeps_its_provider() {
    let git = ProviderId::new("git").expect("provider id");
    let sides = Sides {
        left: Side {
            root: navo_core::Location::with_provider(
                git,
                navo_core::ProviderPath::from_text("repo@main/src"),
            )
            .expect("git location"),
            ..side("left")
        },
        right: side("right"),
    };
    let plan = copy_plan(
        row(Some(file("one.txt", 1)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides,
    )
    .expect("a Git source copies to a local target");
    let (source, _, _) = copy_step(&plan);
    assert_eq!(source.as_str(), "git://repo@main/src/one.txt");
}

#[test]
fn plan_totals_count_files_bytes_and_folders() {
    let plan = copy_plan(
        row(Some(file("one/two/three.bin", 500)), None, Status::OnlyLeft),
        Direction::LeftToRight,
        &sides(),
    )
    .expect("a nested file copies right");
    assert_eq!(plan.totals.files, 1);
    assert_eq!(plan.totals.folders, 2);
    assert_eq!(plan.totals.bytes, 500);
}

#[test]
fn every_planned_copy_has_no_delete_step() {
    let cases = [
        (
            row(Some(file("a.txt", 1)), None, Status::OnlyLeft),
            Direction::LeftToRight,
        ),
        (
            row(None, Some(file("b.txt", 2)), Status::OnlyRight),
            Direction::RightToLeft,
        ),
        (
            row(Some(file("c/d.txt", 3)), None, Status::OnlyLeft),
            Direction::LeftToRight,
        ),
        (
            row(
                Some(file("e.txt", 4)),
                Some(file("e.txt", 5)),
                Status::Different {
                    first_difference: None,
                },
            ),
            Direction::LeftToRight,
        ),
    ];
    for (row, direction) in cases {
        let plan = copy_plan(row, direction, &sides()).expect("a valid copy");
        assert_only_copy_steps(&plan);
    }
}
