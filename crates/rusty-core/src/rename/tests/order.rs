use std::collections::HashMap;

use proptest::prelude::*;

use crate::{Location, OpKind, ProviderPath, Step};

use super::super::check::{Existing, ExistingName, Fold, Problem, Row, RowState, fold_key};
use super::super::order::{BatchPlan, plan_batch};

const SEED: u32 = 42;
const TEMP: &str = ".navo-rename-0000002a";

fn folder(path: &str) -> Location {
    Location::new(path).unwrap()
}

fn at(parent: &str, name: &str) -> Location {
    folder(parent).join(&ProviderPath::from_text(name)).unwrap()
}

fn row_in(parent: &str, old: &str, new: &str) -> Row {
    Row {
        parent: folder(parent),
        old: old.to_owned(),
        new: new.to_owned(),
        excluded: false,
    }
}

fn row(old: &str, new: &str) -> Row {
    row_in("/dir", old, new)
}

fn listing(parent: &str, names: &[&str]) -> Existing {
    Existing::new(
        names
            .iter()
            .map(|name| ExistingName {
                parent: folder(parent),
                name: (*name).to_owned(),
            })
            .collect(),
    )
}

fn plan(rows: &[Row], names: &[&str], fold: Fold) -> BatchPlan {
    plan_batch(rows, &listing("/dir", names), fold, SEED)
}

fn moves(batch: &BatchPlan) -> Vec<(String, String)> {
    let name = |location: &Location| location.as_str().rsplit('/').next().unwrap().to_owned();
    batch
        .plan
        .as_ref()
        .expect("a plan")
        .steps
        .iter()
        .map(|step| match step {
            Step::Rename { source, target } => (name(source), name(target)),
            other => panic!("unexpected step {other:?}"),
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(from, to)| ((*from).to_owned(), (*to).to_owned()))
        .collect()
}

fn temp(n: u32) -> String {
    format!("{TEMP}-{n}")
}

#[test]
fn a_chain_runs_from_the_free_end() {
    let batch = plan(&[row("a", "b"), row("b", "c")], &["a", "b"], Fold::Exact);
    assert_eq!(moves(&batch), pairs(&[("b", "c"), ("a", "b")]));
    let plan = batch.plan.unwrap();
    assert_eq!(plan.kind, OpKind::Rename);
    assert_eq!(plan.totals.files, 2);
    assert!(plan.conflicts.is_empty());
}

#[test]
fn a_long_chain_in_reverse_row_order_still_lands_on_free_names() {
    let rows = [row("c", "d"), row("b", "c"), row("a", "b")];
    let batch = plan(&rows, &["a", "b", "c"], Fold::Exact);
    assert_eq!(moves(&batch), pairs(&[("c", "d"), ("b", "c"), ("a", "b")]));
}

#[test]
fn a_swap_uses_one_temporary_name() {
    let batch = plan(&[row("a", "b"), row("b", "a")], &["a", "b"], Fold::Exact);
    let t = temp(0);
    assert_eq!(moves(&batch), pairs(&[("a", &t), ("b", "a"), (&t, "b")]));
    assert_eq!(batch.plan.unwrap().totals.files, 2);
}

#[test]
fn a_three_cycle_uses_one_temporary_name() {
    let rows = [row("a", "b"), row("b", "c"), row("c", "a")];
    let batch = plan(&rows, &["a", "b", "c"], Fold::Exact);
    let t = temp(0);
    assert_eq!(
        moves(&batch),
        pairs(&[("a", &t), ("c", "a"), ("b", "c"), (&t, "b")])
    );
}

#[test]
fn a_case_only_rename_goes_through_a_temporary_name_when_folding() {
    let batch = plan(&[row("A.txt", "a.txt")], &["A.txt"], Fold::Insensitive);
    let t = temp(0);
    assert_eq!(moves(&batch), pairs(&[("A.txt", &t), (&t, "a.txt")]));
    assert_eq!(batch.plan.unwrap().totals.files, 1);
}

#[test]
fn a_case_only_rename_is_direct_when_names_are_exact() {
    let batch = plan(&[row("a.txt", "A.txt")], &["a.txt"], Fold::Exact);
    assert_eq!(moves(&batch), pairs(&[("a.txt", "A.txt")]));
}

#[test]
fn a_cycle_next_to_a_case_only_rename_gets_its_own_temporary_name() {
    let rows = [row("A.txt", "a.txt"), row("b", "c"), row("c", "b")];
    let batch = plan(&rows, &["A.txt", "b", "c"], Fold::Insensitive);
    let (t0, t1) = (temp(0), temp(1));
    assert_eq!(
        moves(&batch),
        pairs(&[
            ("A.txt", &t0),
            (&t0, "a.txt"),
            ("b", &t1),
            ("c", "b"),
            (&t1, "c"),
        ])
    );
}

#[test]
fn a_swap_that_differs_only_by_case_folds_into_one_cycle() {
    let rows = [row("a", "B"), row("b", "A")];
    let batch = plan(&rows, &["a", "b"], Fold::Insensitive);
    let t = temp(0);
    assert_eq!(moves(&batch), pairs(&[("a", &t), ("b", "A"), (&t, "B")]));
}

#[test]
fn a_temporary_name_in_the_listing_is_skipped() {
    let taken = temp(0);
    let batch = plan(
        &[row("a", "b"), row("b", "a")],
        &["a", "b", &taken],
        Fold::Exact,
    );
    let t = temp(1);
    assert_eq!(moves(&batch), pairs(&[("a", &t), ("b", "a"), (&t, "b")]));
}

#[test]
fn a_temporary_name_used_by_the_batch_is_skipped() {
    let wanted = temp(0).to_uppercase();
    let rows = [row("a", "b"), row("b", "a"), row("c", &wanted)];
    let batch = plan(&rows, &["a", "b", "c"], Fold::Insensitive);
    let steps = moves(&batch);
    assert!(steps.contains(&("a".to_owned(), temp(1))));
    assert!(!steps.iter().any(|(_, to)| *to == temp(0)));
}

#[test]
fn unchanged_and_excluded_rows_give_no_steps() {
    let mut skipped = row("c", "c");
    skipped.excluded = true;
    let rows = [row("a", "a"), row("b", "b"), skipped];
    let batch = plan(&rows, &["a", "b", "c"], Fold::Exact);
    let plan = batch.plan.unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.totals.files, 0);
}

#[test]
fn a_problem_blocks_the_plan_and_keeps_the_states() {
    let rows = [row("a", "b"), row("x", "b")];
    let batch = plan(&rows, &["a", "x"], Fold::Exact);
    assert!(batch.plan.is_none());
    assert_eq!(batch.states[1], RowState::Problem(Problem::Duplicate(0)));
    let exists = plan(&[row("a", "x")], &["a", "x"], Fold::Exact);
    assert!(exists.plan.is_none());
}

#[test]
fn each_folder_is_ordered_on_its_own() {
    let rows = [
        row_in("/one", "a", "b"),
        row_in("/one", "b", "a"),
        row_in("/two", "a", "b"),
        row_in("/two", "b", "a"),
    ];
    let existing = Existing::new(
        ["/one", "/two"]
            .iter()
            .flat_map(|parent| {
                ["a", "b"].map(|name| ExistingName {
                    parent: folder(parent),
                    name: name.to_owned(),
                })
            })
            .collect(),
    );
    let batch = plan_batch(&rows, &existing, Fold::Exact, SEED);
    let steps = batch.plan.unwrap().steps;
    assert_eq!(steps.len(), 6);
    assert_eq!(
        steps[0],
        Step::Rename {
            source: at("/one", "a"),
            target: at("/one", &temp(0)),
        }
    );
    assert_eq!(
        steps[3],
        Step::Rename {
            source: at("/two", "a"),
            target: at("/two", &temp(1)),
        }
    );
}

#[derive(Debug, Clone)]
struct Case {
    rows: Vec<Row>,
    outside: Vec<String>,
    fold: Fold,
}

fn spelled(index: usize, upper: bool) -> String {
    let name = format!("n{index}.txt");
    if upper { name.to_uppercase() } else { name }
}

fn case_strategy() -> impl Strategy<Value = Case> {
    (1usize..7, any::<bool>())
        .prop_flat_map(|(count, insensitive)| {
            let free = count + 2;
            (
                Just((count, insensitive)),
                Just((0..free).collect::<Vec<usize>>()).prop_shuffle(),
                proptest::collection::vec(any::<bool>(), count),
                proptest::collection::vec(any::<bool>(), free),
            )
        })
        .prop_map(|((count, insensitive), targets, old_case, new_case)| Case {
            rows: (0..count)
                .map(|index| {
                    row(
                        &spelled(index, old_case[index]),
                        &spelled(targets[index], new_case[targets[index]]),
                    )
                })
                .collect(),
            outside: vec!["keep.txt".to_owned(), "Other.txt".to_owned()],
            fold: if insensitive {
                Fold::Insensitive
            } else {
                Fold::Exact
            },
        })
}

struct Folder {
    entries: HashMap<String, String>,
    fold: Fold,
}

impl Folder {
    fn new(names: impl IntoIterator<Item = String>, fold: Fold) -> Self {
        let entries = names
            .into_iter()
            .map(|name| (fold_key(&name, fold), name))
            .collect();
        Self { entries, fold }
    }

    fn apply(&mut self, from: &str, to: &str) -> Result<(), String> {
        let source = fold_key(from, self.fold);
        let target = fold_key(to, self.fold);
        let exact = self.entries.get(&source).is_some_and(|name| name == from);
        if !exact {
            return Err(format!("no entry {from}"));
        }
        if self.entries.contains_key(&target) {
            return Err(format!("{to} is taken"));
        }
        self.entries.remove(&source);
        self.entries.insert(target, to.to_owned());
        Ok(())
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.entries.values().cloned().collect();
        names.sort();
        names
    }
}

proptest! {
    #[test]
    fn steps_land_on_free_names_and_reach_the_wanted_state(case in case_strategy()) {
        let names: Vec<&str> = case
            .rows
            .iter()
            .map(|row| row.old.as_str())
            .chain(case.outside.iter().map(String::as_str))
            .collect();
        let batch = plan(&case.rows, &names, case.fold);
        prop_assume!(batch.plan.is_some());
        let mut model = Folder::new(names.iter().map(|name| (*name).to_owned()), case.fold);
        for (from, to) in moves(&batch) {
            prop_assert_eq!(model.apply(&from, &to), Ok(()));
        }
        let mut wanted: Vec<String> = case
            .rows
            .iter()
            .map(|row| row.new.clone())
            .chain(case.outside.iter().cloned())
            .collect();
        wanted.sort();
        prop_assert_eq!(model.names(), wanted);
    }

    #[test]
    fn a_plan_exists_exactly_when_no_row_has_a_problem(case in case_strategy()) {
        let names: Vec<&str> = case.rows.iter().map(|row| row.old.as_str()).collect();
        let batch = plan(&case.rows, &names, case.fold);
        let blocked = batch.states.iter().any(|state| matches!(state, RowState::Problem(_)));
        prop_assert_eq!(batch.plan.is_none(), blocked);
    }
}
