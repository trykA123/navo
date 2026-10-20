use std::collections::{HashMap, HashSet};

use crate::{Location, OpKind, OperationPlan, ProviderPath, Step, Totals};

use super::check::{Existing, Fold, Key, Row, RowState, check_rows, key};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    pub states: Vec<RowState>,
    pub plan: Option<OperationPlan>,
}

pub fn plan_batch(rows: &[Row], existing: &Existing, fold: Fold, seed: u32) -> BatchPlan {
    let states = check_rows(rows, existing, fold);
    let blocked = states
        .iter()
        .any(|state| matches!(state, RowState::Problem(_)));
    let plan = if blocked {
        None
    } else {
        let changing: Vec<&Row> = rows
            .iter()
            .zip(&states)
            .filter(|(_, state)| matches!(state, RowState::Changes { .. }))
            .map(|(row, _)| row)
            .collect();
        build_plan(&changing, rows, existing, Order { fold, seed })
    };
    BatchPlan { states, plan }
}

#[derive(Clone, Copy)]
struct Order {
    fold: Fold,
    seed: u32,
}

struct Move {
    parent: Location,
    from: String,
    to: String,
}

fn build_plan(
    changing: &[&Row],
    rows: &[Row],
    existing: &Existing,
    order: Order,
) -> Option<OperationPlan> {
    let moves = Orderer::new(changing, rows, existing, order).run();
    let steps = moves.iter().map(step).collect::<Option<Vec<_>>>()?;
    Some(OperationPlan {
        kind: OpKind::Rename,
        steps,
        totals: Totals {
            files: u64::try_from(changing.len()).unwrap_or(u64::MAX),
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    })
}

fn step(item: &Move) -> Option<Step> {
    let at = |name: &str| item.parent.join(&ProviderPath::from_text(name)).ok();
    Some(Step::Rename {
        source: at(&item.from)?,
        target: at(&item.to)?,
    })
}

struct Orderer<'a> {
    rows: &'a [&'a Row],
    order: Order,
    sources: HashSet<Key>,
    by_target: HashMap<Key, usize>,
    taken: HashSet<Key>,
    done: Vec<bool>,
    next_temp: u32,
    moves: Vec<Move>,
}

impl<'a> Orderer<'a> {
    fn new(changing: &'a [&'a Row], rows: &[Row], existing: &Existing, order: Order) -> Self {
        let fold = order.fold;
        let sources = changing
            .iter()
            .map(|row| key(&row.parent, &row.old, fold))
            .collect();
        let by_target = changing
            .iter()
            .enumerate()
            .map(|(index, row)| (key(&row.parent, &row.new, fold), index))
            .collect();
        let listed = existing
            .names
            .iter()
            .map(|item| key(&item.parent, &item.name, fold));
        let batch = rows.iter().flat_map(|row| {
            [
                key(&row.parent, &row.old, fold),
                key(&row.parent, &row.new, fold),
            ]
        });
        Self {
            rows: changing,
            order,
            sources,
            by_target,
            taken: listed.chain(batch).collect(),
            done: vec![false; changing.len()],
            next_temp: 0,
            moves: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Move> {
        for index in 0..self.rows.len() {
            if !self.sources.contains(&self.target_key(index)) {
                self.chain(index);
            }
        }
        for index in 0..self.rows.len() {
            if !self.done[index] {
                self.cycle(index);
            }
        }
        self.moves
    }

    fn source_key(&self, index: usize) -> Key {
        let row = self.rows[index];
        key(&row.parent, &row.old, self.order.fold)
    }

    fn target_key(&self, index: usize) -> Key {
        let row = self.rows[index];
        key(&row.parent, &row.new, self.order.fold)
    }

    fn blocked_by_me(&self, index: usize) -> Option<usize> {
        self.by_target
            .get(&self.source_key(index))
            .copied()
            .filter(|&other| !self.done[other])
    }

    fn direct(&mut self, index: usize) {
        let row = self.rows[index];
        self.done[index] = true;
        self.moves.push(Move {
            parent: row.parent.clone(),
            from: row.old.clone(),
            to: row.new.clone(),
        });
    }

    fn chain(&mut self, head: usize) {
        let mut current = Some(head);
        while let Some(index) = current {
            self.direct(index);
            current = self.blocked_by_me(index);
        }
    }

    fn cycle(&mut self, start: usize) {
        let row = self.rows[start];
        let temp = self.temp_name(&row.parent);
        self.done[start] = true;
        self.moves.push(Move {
            parent: row.parent.clone(),
            from: row.old.clone(),
            to: temp.clone(),
        });
        let mut current = self.blocked_by_me(start);
        while let Some(index) = current {
            self.direct(index);
            current = self.blocked_by_me(index);
        }
        self.moves.push(Move {
            parent: row.parent.clone(),
            from: temp,
            to: row.new.clone(),
        });
    }

    fn temp_name(&mut self, parent: &Location) -> String {
        loop {
            let name = format!(".ferrite-rename-{:08x}-{}", self.order.seed, self.next_temp);
            self.next_temp += 1;
            if self.taken.insert(key(parent, &name, self.order.fold)) {
                return name;
            }
        }
    }
}
