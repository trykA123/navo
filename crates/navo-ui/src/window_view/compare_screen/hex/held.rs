//! What a hex body keeps on screen while its new bytes are on the way (V39): the old row map
//! after an alignment switch, or the whole drawn state of the old view after a refresh.

use std::ops::Range;

use super::HexView;
use super::fetch::ByteCache;
use super::hex_rows::Rows;

/// The row map still drawn, and for a refresh the bytes and tints that went with it.
pub(super) struct Held {
    rows: Rows,
    /// The old view's own bytes and marks; none when the held map shares this view's cache.
    carried: Option<Carried>,
}

/// The drawn state of a view that a refresh replaced. Its byte cache stays here: the new
/// view's cache starts empty, since the file may have changed under the old span keys.
struct Carried {
    loaded: ByteCache,
    left_marks: Vec<(Range<u64>, usize)>,
    right_marks: Vec<(Range<u64>, usize)>,
}

/// The rows, bytes and tints one frame draws with.
pub(super) struct Drawn<'a> {
    pub(super) rows: &'a Rows,
    pub(super) loaded: &'a ByteCache,
    pub(super) left: &'a [(Range<u64>, usize)],
    pub(super) right: &'a [(Range<u64>, usize)],
    pub(super) current: Option<usize>,
}

impl Held {
    /// A map held after an alignment switch; it reads the view's own bytes and marks.
    pub(super) fn shared(rows: Rows) -> Self {
        Self {
            rows,
            carried: None,
        }
    }

    /// Whether this holds a replaced view's whole drawn state.
    pub(super) fn is_carried(&self) -> bool {
        self.carried.is_some()
    }
}

impl HexView {
    /// A hex body for a reopened pair. The old view's drawn state and scroll position stay
    /// on screen until the new view has read its visible rows; a first open has nothing.
    pub(in crate::window_view::compare_screen) fn reopened(old: Option<HexView>) -> Self {
        let Some(old) = old else {
            return Self::new(None, 0);
        };
        let mut view = Self::new(old.user_align, old.run);
        view.top = old.top;
        view.held = old.into_held();
        view
    }

    /// The old view's drawn state as one held value; none when it drew no rows.
    fn into_held(self) -> Option<Held> {
        let held = match self.held {
            Some(held) if held.is_carried() => held,
            shared => Held {
                rows: shared.map_or(self.rows, |held| held.rows),
                carried: Some(Carried {
                    loaded: self.loaded,
                    left_marks: self.left_marks,
                    right_marks: self.right_marks,
                }),
            },
        };
        (held.rows.count() > 0).then_some(held)
    }

    /// Whether the open of both sides has landed, with sources or with a failure.
    pub(super) fn installed(&self) -> bool {
        self.left.is_some() || self.right.is_some() || self.error.is_some()
    }

    /// Whether the status line would describe a pair that is not on screen yet.
    pub(super) fn shows_old_pair(&self) -> bool {
        self.held.as_ref().is_some_and(Held::is_carried)
    }

    /// The rows the scroll position counts against: the held map until the open lands.
    pub(super) fn row_count(&self) -> usize {
        match &self.held {
            Some(held) if !self.installed() => held.rows.count(),
            _ => self.rows.count(),
        }
    }

    /// What to draw for `range`: the held map before the open lands and until the current
    /// map's visible bytes are read, else the current map.
    pub(super) fn drawn(&self, range: &Range<usize>) -> Drawn<'_> {
        let own = Drawn {
            rows: &self.rows,
            loaded: &self.loaded,
            left: &self.left_marks,
            right: &self.right_marks,
            current: self.current,
        };
        let Some(held) = &self.held else {
            return own;
        };
        if self.installed() && !self.unfetched(range) {
            return own;
        }
        match &held.carried {
            Some(old) => Drawn {
                rows: &held.rows,
                loaded: &old.loaded,
                left: &old.left_marks,
                right: &old.right_marks,
                current: None,
            },
            None => Drawn {
                rows: &held.rows,
                ..own
            },
        }
    }

    /// Drops the held map once `range` has its bytes; kept while the open is still to land.
    pub(super) fn release_held(&mut self, range: &Range<usize>) {
        if self.installed() && !self.unfetched(range) {
            self.held = None;
        }
    }
}
