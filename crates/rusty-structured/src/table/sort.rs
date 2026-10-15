use std::cmp::Ordering;

use super::sniff::is_number;

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

fn row_ids(rows: &[Vec<String>]) -> impl Iterator<Item = (usize, u32)> {
    (0..rows.len()).filter_map(|row| u32::try_from(row).ok().map(|id| (row, id)))
}

#[must_use]
pub fn sort_order(rows: &[Vec<String>], column: usize, descending: bool) -> Vec<u32> {
    let direction = |ordering: Ordering| {
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    };
    if is_numeric_column(rows, column) {
        let keyed = row_ids(rows)
            .map(|(row, id)| (cell(rows, row, column).parse::<f64>().ok(), id))
            .collect();
        return ordered(keyed, |left, right| match (left, right) {
            (Some(left), Some(right)) => direction(left.total_cmp(right)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });
    }
    let keyed = row_ids(rows)
        .map(|(row, id)| (cell(rows, row, column).to_lowercase(), id))
        .collect();
    ordered(keyed, |left: &String, right: &String| {
        match (left.is_empty(), right.is_empty()) {
            (false, true) => Ordering::Less,
            (true, false) => Ordering::Greater,
            _ => direction(left.cmp(right)),
        }
    })
}
