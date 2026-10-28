use chrono::{NaiveDate, TimeZone};
use rusty_core::retention::RetentionId;

use crate::browser::RetainedRow;

pub const PAGE_ROWS: u16 = 200;
pub const MAX_ROWS: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Day {
    Today,
    Yesterday,
    On(NaiveDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayHead {
    pub day: Day,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Day(DayHead),
    Item(RetainedRow),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    entries: Vec<Entry>,
    items: usize,
    bytes: u64,
}

fn date_of<Zone: TimeZone>(at_ms: i64, zone: &Zone) -> NaiveDate {
    zone.timestamp_millis_opt(at_ms)
        .single()
        .map(|moment| moment.date_naive())
        .unwrap_or_default()
}

fn day_of(date: NaiveDate, today: NaiveDate) -> Day {
    if date == today {
        Day::Today
    } else if today.pred_opt() == Some(date) {
        Day::Yesterday
    } else {
        Day::On(date)
    }
}

impl Model {
    pub fn new<Zone: TimeZone>(rows: &[RetainedRow], now_ms: i64, zone: &Zone) -> Self {
        let mut sorted: Vec<&RetainedRow> = rows.iter().collect();
        sorted.sort_by_key(|row| std::cmp::Reverse((row.deleted_at_ms, row.id)));
        let today = date_of(now_ms, zone);
        let mut entries: Vec<Entry> = Vec::with_capacity(sorted.len() + 8);
        let mut head_at: Option<usize> = None;
        for row in sorted {
            let day = day_of(date_of(row.deleted_at_ms, zone), today);
            let open = head_at.and_then(|at| match entries.get_mut(at) {
                Some(Entry::Day(head)) if head.day == day => Some(head),
                _ => None,
            });
            match open {
                Some(head) => {
                    head.count += 1;
                    head.bytes = head.bytes.saturating_add(row.bytes);
                }
                None => {
                    head_at = Some(entries.len());
                    entries.push(Entry::Day(DayHead {
                        day,
                        count: 1,
                        bytes: row.bytes,
                    }));
                }
            }
            entries.push(Entry::Item(row.clone()));
        }
        Self {
            items: rows.len(),
            bytes: rows
                .iter()
                .fold(0, |sum, row| sum.saturating_add(row.bytes)),
            entries,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn items(&self) -> usize {
        self.items
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.items == 0
    }

    pub fn row_at(&self, index: usize) -> Option<&RetainedRow> {
        match self.entries.get(index)? {
            Entry::Item(row) => Some(row),
            Entry::Day(_) => None,
        }
    }

    pub fn index_of(&self, id: RetentionId) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| matches!(entry, Entry::Item(row) if row.id == id))
    }

    pub fn first_item(&self) -> Option<usize> {
        (0..self.entries.len()).find(|at| self.row_at(*at).is_some())
    }

    pub fn step(&self, from: usize, forward: bool) -> usize {
        let next = if forward {
            (from + 1..self.entries.len()).find(|at| self.row_at(*at).is_some())
        } else {
            (0..from).rev().find(|at| self.row_at(*at).is_some())
        };
        next.or_else(|| self.row_at(from).map(|_| from))
            .or_else(|| self.first_item())
            .unwrap_or(0)
    }
}
