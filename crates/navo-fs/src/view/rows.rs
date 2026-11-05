use std::cell::Cell;
use std::ops::Index;
use std::sync::{Arc, Weak};

use crate::FsEntry;

/// A row set over one folder generation.
///
/// `order` holds source indices; `None` shows the source as it is.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    source: Arc<[FsEntry]>,
    order: Option<Arc<[u32]>>,
}

/// A row set's source without keeping it alive.
#[derive(Clone, Debug)]
pub struct WeakRows(Weak<[FsEntry]>);

impl Rows {
    #[must_use]
    pub fn all(source: Arc<[FsEntry]>) -> Self {
        Self {
            source,
            order: None,
        }
    }

    #[must_use]
    pub fn ordered(source: Arc<[FsEntry]>, order: Arc<[u32]>) -> Self {
        debug_assert!(
            order
                .iter()
                .all(|&index| usize::try_from(index).is_ok_and(|at| at < source.len())),
            "row order must index the source"
        );
        Self {
            source,
            order: Some(order),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.order
            .as_ref()
            .map_or(self.source.len(), |order| order.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn get(&self, at: usize) -> Option<&FsEntry> {
        match &self.order {
            Some(order) => self.source.get(*order.get(at)? as usize),
            None => self.source.get(at),
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &FsEntry> + DoubleEndedIterator {
        let inner = match &self.order {
            Some(order) => RowsIterInner::Ordered {
                source: &self.source,
                order: order.iter(),
            },
            None => RowsIterInner::All(self.source.iter()),
        };
        RowsIter { inner }
    }

    #[must_use]
    pub fn source(&self) -> &Arc<[FsEntry]> {
        &self.source
    }

    #[must_use]
    pub fn order(&self) -> Option<&Arc<[u32]>> {
        self.order.as_ref()
    }

    /// Narrows this view to `keep`, source indices as `filter_order` returns them.
    #[must_use]
    pub fn narrowed(&self, keep: Vec<u32>) -> Self {
        debug_assert!(
            keep.iter()
                .all(|&index| usize::try_from(index).is_ok_and(|at| at < self.source.len())),
            "narrowed rows must index the source"
        );
        Self {
            source: Arc::clone(&self.source),
            order: Some(keep.into()),
        }
    }

    #[must_use]
    pub fn same(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.source, &right.source)
            && match (&left.order, &right.order) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                _ => false,
            }
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<FsEntry> {
        let mut rows = Vec::with_capacity(self.len());
        for entry in self.iter() {
            rows.push(entry.clone());
            count_copied_row(entry);
        }
        rows
    }

    #[must_use]
    pub fn to_arc(&self) -> Arc<[FsEntry]> {
        match self.order {
            None => Arc::clone(&self.source),
            Some(_) => self.to_vec().into(),
        }
    }

    #[must_use]
    pub fn downgrade(&self) -> WeakRows {
        WeakRows(Arc::downgrade(&self.source))
    }
}

impl WeakRows {
    #[must_use]
    pub fn is_view_of(&self, rows: &Rows) -> bool {
        self.0
            .upgrade()
            .is_some_and(|source| Arc::ptr_eq(&source, &rows.source))
    }
}

impl Index<usize> for Rows {
    type Output = FsEntry;

    fn index(&self, at: usize) -> &Self::Output {
        match &self.order {
            Some(order) => &self.source[order[at] as usize],
            None => &self.source[at],
        }
    }
}

impl From<Arc<[FsEntry]>> for Rows {
    fn from(source: Arc<[FsEntry]>) -> Self {
        Self::all(source)
    }
}

impl From<Vec<FsEntry>> for Rows {
    fn from(rows: Vec<FsEntry>) -> Self {
        Self::all(rows.into())
    }
}

impl FromIterator<FsEntry> for Rows {
    fn from_iter<T: IntoIterator<Item = FsEntry>>(iter: T) -> Self {
        Self::all(iter.into_iter().collect())
    }
}

struct RowsIter<'a> {
    inner: RowsIterInner<'a>,
}

enum RowsIterInner<'a> {
    All(std::slice::Iter<'a, FsEntry>),
    Ordered {
        source: &'a [FsEntry],
        order: std::slice::Iter<'a, u32>,
    },
}

impl<'a> Iterator for RowsIter<'a> {
    type Item = &'a FsEntry;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            RowsIterInner::All(iter) => iter.next(),
            RowsIterInner::Ordered { source, order } => {
                let index = *order.next()?;
                Some(&source[index as usize])
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for RowsIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            RowsIterInner::All(iter) => iter.next_back(),
            RowsIterInner::Ordered { source, order } => {
                let index = *order.next_back()?;
                Some(&source[index as usize])
            }
        }
    }
}

impl ExactSizeIterator for RowsIter<'_> {
    fn len(&self) -> usize {
        match &self.inner {
            RowsIterInner::All(iter) => iter.len(),
            RowsIterInner::Ordered { order, .. } => order.len(),
        }
    }
}

thread_local! {
    static COPIED_ROW_BYTES: Cell<usize> = const { Cell::new(0) };
}

fn row_bytes(entry: &FsEntry) -> usize {
    size_of::<FsEntry>() + entry.name.display().len()
}

/// Counts each row this thread clones out of a row set.
pub(crate) fn count_copied_row(entry: &FsEntry) {
    COPIED_ROW_BYTES.with(|bytes| bytes.set(bytes.get() + row_bytes(entry)));
}

/// Row bytes cloned on this thread by materializing a row set.
#[doc(hidden)]
#[must_use]
pub fn copied_row_bytes() -> usize {
    COPIED_ROW_BYTES.with(Cell::get)
}

#[doc(hidden)]
pub fn reset_copied_row_bytes() {
    COPIED_ROW_BYTES.with(|bytes| bytes.set(0));
}

#[cfg(test)]
mod tests;
