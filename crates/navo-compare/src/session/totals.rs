use crate::model::{Row, Status};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub rows: u64,
    pub same: u64,
    pub different: u64,
    pub only_left: u64,
    pub only_right: u64,
    pub conflicts: u64,
    pub checking: u64,
    pub same_size_time: u64,
    pub unavailable: u64,
    pub not_checked: u64,
}

impl Totals {
    pub(super) fn record(&mut self, status: Status, first: bool) {
        if first {
            self.rows = self.rows.saturating_add(1);
        }
        match status {
            Status::Same | Status::SameAfterRules => {
                self.same = self.same.saturating_add(1);
            }
            Status::Different { .. } => {
                self.different = self.different.saturating_add(1);
            }
            Status::OnlyLeft => {
                self.only_left = self.only_left.saturating_add(1);
            }
            Status::OnlyRight => {
                self.only_right = self.only_right.saturating_add(1);
            }
            Status::TypeConflict | Status::CaseConflict => {
                self.conflicts = self.conflicts.saturating_add(1);
            }
            Status::Checking => {
                self.checking = self.checking.saturating_add(1);
            }
            Status::SameSizeTime => {
                self.same_size_time = self.same_size_time.saturating_add(1);
            }
            Status::Unavailable(_) => {
                self.unavailable = self.unavailable.saturating_add(1);
            }
            Status::NotChecked(_) => {
                self.not_checked = self.not_checked.saturating_add(1);
            }
        }
    }

    pub(super) fn resolve(&mut self, status: Status) {
        self.checking = self.checking.saturating_sub(1);
        self.record(status, false);
    }

    pub(super) fn tally(rows: &[Row]) -> Self {
        let mut totals = Self::default();
        for row in rows {
            totals.record(row.raw_status, true);
        }
        totals
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.rows = self.rows.saturating_add(other.rows);
        self.same = self.same.saturating_add(other.same);
        self.different = self.different.saturating_add(other.different);
        self.only_left = self.only_left.saturating_add(other.only_left);
        self.only_right = self.only_right.saturating_add(other.only_right);
        self.conflicts = self.conflicts.saturating_add(other.conflicts);
        self.checking = self.checking.saturating_add(other.checking);
        self.same_size_time = self.same_size_time.saturating_add(other.same_size_time);
        self.unavailable = self.unavailable.saturating_add(other.unavailable);
        self.not_checked = self.not_checked.saturating_add(other.not_checked);
    }

    pub(super) fn replace(&mut self, previous: Status, next: Status) {
        self.remove(previous);
        self.record(next, false);
    }

    fn remove(&mut self, status: Status) {
        match status {
            Status::Same | Status::SameAfterRules => {
                self.same = self.same.saturating_sub(1);
            }
            Status::Different { .. } => {
                self.different = self.different.saturating_sub(1);
            }
            Status::OnlyLeft => {
                self.only_left = self.only_left.saturating_sub(1);
            }
            Status::OnlyRight => {
                self.only_right = self.only_right.saturating_sub(1);
            }
            Status::TypeConflict | Status::CaseConflict => {
                self.conflicts = self.conflicts.saturating_sub(1);
            }
            Status::Checking => {
                self.checking = self.checking.saturating_sub(1);
            }
            Status::SameSizeTime => {
                self.same_size_time = self.same_size_time.saturating_sub(1);
            }
            Status::Unavailable(_) => {
                self.unavailable = self.unavailable.saturating_sub(1);
            }
            Status::NotChecked(_) => {
                self.not_checked = self.not_checked.saturating_sub(1);
            }
        }
    }
}
