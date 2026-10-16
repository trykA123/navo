use std::ops::Range;
use std::sync::Arc;

use rusty_structured::table::{RecordIndex, Table};

use crate::window_view::viewer::background::Job;

pub(super) struct Fetched {
    pub start: u64,
    pub end: u64,
    pub rows: Vec<Vec<String>>,
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
        let covered = indexed
            .fetched
            .as_ref()
            .is_some_and(|fetched| fetched.start <= wanted.start && fetched.end >= wanted.end);
        let asked = indexed
            .fetching
            .as_ref()
            .is_some_and(|(range, _)| range.start <= wanted.start && range.end >= wanted.end);
        !covered && !asked
    }
}
