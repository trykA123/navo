use std::sync::Arc;

use navo_core::Location;

use crate::settings::Values;

/// What one side of a compare row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareKind {
    File,
    Folder,
    Link,
    Submodule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompareSide {
    pub kind: CompareKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

/// A row status. `NotChecked` and `Unavailable` carry the l10n id of their reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    Same,
    SameAfterRules,
    Different,
    OnlyLeft,
    OnlyRight,
    TypeConflict,
    CaseConflict,
    Checking,
    SameSizeTime,
    NotChecked(&'static str),
    Unavailable(&'static str),
}

impl RowStatus {
    /// A row that next and previous difference stop on.
    #[must_use]
    pub fn is_difference(self) -> bool {
        matches!(
            self,
            Self::Different
                | Self::OnlyLeft
                | Self::OnlyRight
                | Self::TypeConflict
                | Self::CaseConflict
        )
    }

    #[must_use]
    pub fn is_same(self) -> bool {
        matches!(self, Self::Same | Self::SameAfterRules | Self::SameSizeTime)
    }
}

/// One aligned row. `path` is relative to the compare roots, joined with `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareRow {
    pub id: u64,
    pub path: String,
    pub left: Option<CompareSide>,
    pub right: Option<CompareSide>,
    pub status: RowStatus,
    /// Folder rows: the subtree holds a difference, whatever the folder's own status is.
    pub contains_differences: bool,
    /// Folder rows: differing files and conflicts below this folder; 0 for files.
    pub differences_inside: u32,
}

impl CompareRow {
    /// A row with no differences below it; set `contains_differences` for folders that have some.
    #[must_use]
    pub fn new(
        id: u64,
        path: impl Into<String>,
        status: RowStatus,
        (left, right): (Option<CompareSide>, Option<CompareSide>),
    ) -> Self {
        Self {
            id,
            path: path.into(),
            left,
            right,
            status,
            contains_differences: false,
            differences_inside: 0,
        }
    }

    /// Whether the Differences filter and next and previous difference stop on this row.
    #[must_use]
    pub fn is_difference(&self) -> bool {
        self.contains_differences || self.status.is_difference()
    }

    /// The folder the row is listed in; the roots' children have `""`.
    #[must_use]
    pub fn parent(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(parent, _)| parent)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map_or(&self.path, |(_, name)| name)
    }

    #[must_use]
    pub fn is_folder_pair(&self) -> bool {
        let folder =
            |side: Option<CompareSide>| side.is_some_and(|s| s.kind == CompareKind::Folder);
        folder(self.left) && folder(self.right)
    }

    #[must_use]
    pub fn has_file(&self) -> bool {
        let file = |side: Option<CompareSide>| side.is_some_and(|s| s.kind == CompareKind::File);
        file(self.left) || file(self.right)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompareTotals {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProgress {
    pub id: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Which bound stopped a walk on a network drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBound {
    Entries,
    Time,
}

/// What a running compare reports. Per generation: rows and totals, then `Finished`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareUpdate {
    Rows {
        generation: u64,
        rows: Vec<CompareRow>,
        progress: Vec<RowProgress>,
    },
    Totals {
        generation: u64,
        totals: CompareTotals,
    },
    Finished {
        generation: u64,
    },
    /// The walk waits at the network limit; the rows found stay. `entries` were listed so far.
    Stopped {
        generation: u64,
        entries: u64,
        bound: StopBound,
    },
    /// The walk left the stopped state after `CompareControl::continue_walk`.
    Continued {
        generation: u64,
    },
    /// The compare did not start; nothing follows. The text is shown to the user.
    Failed(String),
}

/// Receives updates on any thread; it wakes the window.
pub type CompareSink = Arc<dyn Fn(CompareUpdate) + Send + Sync>;

/// A running compare. No call blocks; dropping it cancels without blocking.
pub trait CompareControl: Send {
    fn focus(&self, row_ids: Vec<u64>);
    fn stop_row(&self, id: u64);
    fn stop_all(&self);
    fn refresh(&self);
    /// Resumes a walk that stopped at the network limit.
    fn continue_walk(&self);
}

/// Starts folder compares; the app implements it over the compare engine.
pub trait CompareHooks: Send + Sync {
    /// Whether a compare can read `location`; panes elsewhere do not offer Compare.
    fn supports(&self, location: &Location) -> bool;

    /// Compares `sides.0` (left) with `sides.1` (right) using the compare settings in `values`.
    fn start(
        &self,
        sides: (&Location, &Location),
        values: &Values,
        sink: CompareSink,
    ) -> Box<dyn CompareControl>;
}
