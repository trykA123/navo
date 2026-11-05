use std::cmp::Ordering;
use std::sync::Arc;

use super::Table;
use super::sniff::{is_number, number};

const MEMO_COLUMNS: usize = 8;

fn cell(rows: &[Vec<String>], row: usize, column: usize) -> &str {
    rows.get(row)
        .and_then(|cells| cells.get(column))
        .map_or("", |cell| cell.trim())
}

#[must_use]
pub fn is_numeric_column(rows: &[Vec<String>], column: usize) -> bool {
    let mut cells = (0..rows.len())
        .map(|row| cell(rows, row, column))
        .filter(|cell| !cell.is_empty())
        .peekable();
    cells.peek().is_some() && cells.all(is_number)
}

fn ordered<T>(mut keyed: Vec<(T, u32)>, compare: impl Fn(&T, &T) -> Ordering) -> Vec<u32> {
    keyed.sort_by(|(left, _), (right, _)| compare(left, right));
    keyed.into_iter().map(|(_, row)| row).collect()
}

fn row_ids(rows: usize) -> impl Iterator<Item = (usize, u32)> {
    (0..rows).filter_map(|row| u32::try_from(row).ok().map(|id| (row, id)))
}

fn direction(descending: bool, ordering: Ordering) -> Ordering {
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

enum SortKeys {
    Numbers(Vec<Option<f64>>),
    Text(Vec<String>),
}

struct ColumnKeys {
    column: usize,
    keys: SortKeys,
    used: u64,
}

/// Parsed sort keys for one table, at most [`MEMO_COLUMNS`] columns.
///
/// The keys belong to the table's identity; sorting another table drops them.
#[derive(Default)]
pub struct SortMemo {
    table: Option<Arc<Table>>,
    columns: Vec<ColumnKeys>,
    clock: u64,
}

impl SortMemo {
    /// Sorts row ids by one column, reusing the keys of an earlier sort.
    ///
    /// Ascending and descending share the same keys. The least recently sorted
    /// column is dropped when the memo is full.
    #[must_use]
    pub fn sort_order(&mut self, table: &Arc<Table>, column: usize, descending: bool) -> Vec<u32> {
        if !self
            .table
            .as_ref()
            .is_some_and(|known| Arc::ptr_eq(known, table))
        {
            self.table = Some(table.clone());
            self.columns.clear();
        }
        self.clock = self.clock.wrapping_add(1);
        if let Some(at) = self.columns.iter().position(|keys| keys.column == column) {
            let order = sort_ids(&self.columns[at].keys, table.rows.len(), descending);
            self.columns[at].used = self.clock;
            return order;
        }
        let keys = parse_keys(&table.rows, column);
        let order = sort_ids(&keys, table.rows.len(), descending);
        self.remember(ColumnKeys {
            column,
            keys,
            used: self.clock,
        });
        order
    }

    fn remember(&mut self, keys: ColumnKeys) {
        if self.columns.len() < MEMO_COLUMNS {
            self.columns.push(keys);
        } else if let Some(slot) = self.columns.iter_mut().min_by_key(|keys| keys.used) {
            *slot = keys;
        }
    }
}

fn parse_keys(rows: &[Vec<String>], column: usize) -> SortKeys {
    if let Some(numbers) = numeric_keys(rows, column) {
        return SortKeys::Numbers(numbers);
    }
    SortKeys::Text(
        (0..rows.len())
            .map(|row| {
                count_parsed();
                cell(rows, row, column).to_lowercase()
            })
            .collect(),
    )
}

fn numeric_keys(rows: &[Vec<String>], column: usize) -> Option<Vec<Option<f64>>> {
    let mut keys = Vec::with_capacity(rows.len());
    let mut has_number = false;
    for row in 0..rows.len() {
        let text = cell(rows, row, column);
        if text.is_empty() {
            keys.push(None);
            continue;
        }
        count_parsed();
        let value = number(text)?;
        has_number = true;
        keys.push(Some(value));
    }
    has_number.then_some(keys)
}

fn sort_ids(keys: &SortKeys, rows: usize, descending: bool) -> Vec<u32> {
    match keys {
        SortKeys::Numbers(numbers) => {
            let keyed = row_ids(rows)
                .map(|(row, id)| (numbers.get(row).copied().flatten(), id))
                .collect();
            ordered(keyed, |left, right| match (left, right) {
                (Some(left), Some(right)) => direction(descending, left.total_cmp(right)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
        }
        SortKeys::Text(text) => {
            let keyed = row_ids(rows)
                .filter_map(|(row, id)| text.get(row).map(|key| (key.as_str(), id)))
                .collect();
            ordered(keyed, |left, right| {
                match (left.is_empty(), right.is_empty()) {
                    (false, true) => Ordering::Less,
                    (true, false) => Ordering::Greater,
                    _ => direction(descending, left.cmp(right)),
                }
            })
        }
    }
}

#[cfg(test)]
thread_local! {
    static PARSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_parsed() {
    PARSED.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(super) fn parsed_cells() -> usize {
    PARSED.with(std::cell::Cell::get)
}

#[cfg(not(test))]
fn count_parsed() {}
