use std::ops::Range;
use std::sync::Arc;

use rusty_structured::table::{RecordIndex, Table};

use crate::window_view::viewer::background::Job;

const WINDOW_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct Fetched {
    pub start: u64,
    pub end: u64,
    pub rows: Vec<Vec<String>>,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Indexed {
    pub index: Option<Arc<RecordIndex>>,
    pub scanned: u64,
    pub job: Option<Job>,
    pub fetched: Option<Fetched>,
    pub fetching: Option<(Range<u64>, Job)>,
    pub skip: u64,
}

fn size_of(rows: &[Vec<String>]) -> usize {
    rows.iter().flatten().map(String::len).sum()
}

impl Indexed {
    pub(super) fn continues_at(&self, wanted: &Range<u64>) -> Option<u64> {
        let fetched = self.fetched.as_ref()?;
        (fetched.start <= wanted.start && wanted.start < fetched.end).then_some(fetched.end)
    }

    fn covered_until(&self, wanted: &Range<u64>) -> u64 {
        let mut until = self.continues_at(wanted).unwrap_or(wanted.start);
        if let Some((range, _)) = &self.fetching
            && range.start <= until
            && until < range.end
        {
            until = range.end;
        }
        until
    }

    pub(super) fn store(&mut self, asked: Range<u64>, rows: Vec<Vec<String>>, is_cut: bool) {
        let end = if is_cut {
            asked.start + rows.len() as u64
        } else {
            asked.end
        };
        let bytes = size_of(&rows);
        match &mut self.fetched {
            Some(fetched)
                if fetched.end == asked.start && fetched.bytes + bytes <= WINDOW_BYTES =>
            {
                fetched.rows.extend(rows);
                fetched.end = end;
                fetched.bytes += bytes;
            }
            _ => {
                self.fetched = Some(Fetched {
                    start: asked.start,
                    end,
                    rows,
                    bytes,
                });
            }
        }
    }
}

pub(super) enum Rows {
    Memory {
        table: Arc<Table>,
        order: Option<Arc<Vec<u32>>>,
    },
    Indexed(Box<Indexed>),
}

impl Rows {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Memory { table, .. } => table.rows.len(),
            Self::Indexed(indexed) => indexed.index.as_ref().map_or(0, |index| {
                usize::try_from(index.records.saturating_sub(indexed.skip)).unwrap_or(usize::MAX)
            }),
        }
    }

    pub(super) fn get(&self, row: usize) -> Option<(u64, &[String])> {
        match self {
            Self::Memory { table, order } => {
                let original = match order {
                    Some(order) => *order.get(row)? as usize,
                    None => row,
                };
                let cells = table.rows.get(original)?;
                Some((original as u64 + 1, cells.as_slice()))
            }
            Self::Indexed(indexed) => {
                let fetched = indexed.fetched.as_ref()?;
                let at = (row as u64).checked_sub(fetched.start)?;
                let cells = fetched.rows.get(usize::try_from(at).ok()?)?;
                Some((row as u64 + 1, cells.as_slice()))
            }
        }
    }

    pub(super) fn missing(&self, wanted: &Range<u64>) -> bool {
        let Self::Indexed(indexed) = self else {
            return false;
        };
        if indexed.index.is_none() || wanted.is_empty() {
            return false;
        }
        indexed.covered_until(wanted) < wanted.end
    }
}

#[cfg(test)]
mod tests {
    use super::{Indexed, Rows, WINDOW_BYTES};

    #[test]
    fn a_full_window_is_replaced_and_every_fetched_row_stays_readable() {
        let mut indexed = Indexed::default();
        let big = "x".repeat(WINDOW_BYTES / 3 + 1);
        for row in 0..8u64 {
            indexed.store(row..row + 4, vec![vec![big.clone()]], true);
            let rows = Rows::Indexed(Box::new(indexed));
            let (number, cells) = rows.get(row as usize).expect("the new row is readable");
            assert_eq!((number, cells[0].len()), (row + 1, big.len()));
            let Rows::Indexed(back) = rows else {
                unreachable!("built as indexed above")
            };
            indexed = *back;
        }
    }
}
