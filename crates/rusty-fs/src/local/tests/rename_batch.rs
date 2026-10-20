use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;

use super::ops::{Log, child};
use super::*;
use crate::job::{JobContext, Progress, RenameOutcome, run_rename};

fn rename_all(root: &Location, pairs: &[(&str, &str)]) -> RenameOutcome {
    let ops = LocalFs::new();
    let rows: Vec<Row> = pairs
        .iter()
        .map(|(old, new)| Row {
            parent: root.clone(),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            excluded: false,
        })
        .collect();
    let names = sorted_names(&list_all(root).1)
        .into_iter()
        .map(|name| ExistingName {
            parent: root.clone(),
            name,
        })
        .collect();
    let plan = plan_batch(&rows, &Existing::new(names), Fold::Insensitive, 1)
        .plan
        .expect("a plan");
    let cancel = CancelToken::new();
    let mut log = Log::default();
    let mut on_progress = |_: &Progress| {};
    let mut ctx = JobContext {
        ops: &ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut on_progress,
    };
    run_rename(&plan, &mut ctx)
}

fn sizes(root: &Location, names: &[&str]) -> Vec<Option<u64>> {
    names
        .iter()
        .map(|name| {
            LocalFs::new()
                .metadata(&child(root, name))
                .ok()
                .map(|entry| entry.size)
        })
        .collect()
}

#[test]
fn a_case_only_rename_changes_the_spelling() {
    let (dir, root) = temp();
    write_file(dir.path(), "report.txt", 3);
    assert_eq!(
        rename_all(&root, &[("report.txt", "Report.txt")]),
        RenameOutcome::Done
    );
    assert_eq!(sorted_names(&list_all(&root).1), ["Report.txt"]);
}

#[test]
fn two_names_swap() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    write_file(dir.path(), "b.txt", 5);
    let swap = [("a.txt", "b.txt"), ("b.txt", "a.txt")];
    assert_eq!(rename_all(&root, &swap), RenameOutcome::Done);
    assert_eq!(sizes(&root, &["a.txt", "b.txt"]), [Some(5), Some(3)]);
    assert_eq!(sorted_names(&list_all(&root).1), ["a.txt", "b.txt"]);
}

#[test]
fn a_swap_with_a_case_change_keeps_every_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "a.txt", 3);
    write_file(dir.path(), "b.txt", 5);
    let swap = [("a.txt", "B.txt"), ("b.txt", "A.txt")];
    assert_eq!(rename_all(&root, &swap), RenameOutcome::Done);
    assert_eq!(sorted_names(&list_all(&root).1), ["A.txt", "B.txt"]);
    assert_eq!(sizes(&root, &["A.txt", "B.txt"]), [Some(5), Some(3)]);
}
