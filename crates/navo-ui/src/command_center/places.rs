//! Place lookup for the command center: finds the first kept location that is the same place
//! as a new one with one hash lookup, under the rules of `Location::same_place`.

#[cfg(test)]
use std::cell::Cell;
use std::collections::HashMap;

use navo_core::{Location, ProviderId, ProviderPath, WindowsFoldKey};

#[cfg(test)]
thread_local! {
    static FOLD_KEYS: Cell<usize> = const { Cell::new(0) };
}

/// Counts `n` case-folded path keys built on this thread (tests read it to prove linear cost).
#[cfg(test)]
fn count_fold_keys(n: usize) {
    FOLD_KEYS.with(|keys| keys.set(keys.get() + n));
}

/// Returns the case-folded path keys built on this thread since the last call, then resets it.
#[cfg(test)]
pub(super) fn take_fold_keys() -> usize {
    FOLD_KEYS.with(Cell::take)
}

/// Builds the case-folded key of a location's path; the one costly step of a lookup.
fn fold_key(location: &Location) -> WindowsFoldKey {
    #[cfg(test)]
    count_fold_keys(1);
    location.path().windows_fold_key()
}

/// A kept value and its insertion rank, so the earliest of two candidates wins.
struct Kept<T> {
    rank: usize,
    value: T,
}

/// A location ready for `Places::find` and `Places::insert`; its folded key is built once.
pub(super) struct PlaceKey<'a> {
    location: &'a Location,
    folded: Option<WindowsFoldKey>,
}

/// A hash index of kept locations. `find(b)` returns the value of the first kept `a` with
/// `a.same_place(b)`: Windows-style local paths fold case, every other path matches exactly.
pub(super) struct Places<T> {
    folded: HashMap<ProviderId, HashMap<WindowsFoldKey, Kept<T>>>,
    exact: HashMap<ProviderId, HashMap<ProviderPath, Kept<T>>>,
    len: usize,
}

impl<T> Default for Places<T> {
    fn default() -> Self {
        Self {
            folded: HashMap::new(),
            exact: HashMap::new(),
            len: 0,
        }
    }
}

impl<T: Copy> Places<T> {
    /// Prepares `location` for a lookup; folds its path only when a fold can matter.
    pub(super) fn key<'a>(&self, location: &'a Location) -> PlaceKey<'a> {
        let needs_fold = location.folds_case() || self.folded.contains_key(location.provider());
        PlaceKey {
            location,
            folded: needs_fold.then(|| fold_key(location)),
        }
    }

    /// Returns the value of the earliest kept location that is the same place as `key`.
    pub(super) fn find(&self, key: &PlaceKey<'_>) -> Option<T> {
        let location = key.location;
        let folded = self
            .folded
            .get(location.provider())
            .and_then(|keys| match &key.folded {
                Some(folded) => keys.get(folded),
                None => keys.get(&fold_key(location)),
            });
        let exact = self
            .exact
            .get(location.provider())
            .and_then(|paths| paths.get(location.path()));
        [folded, exact]
            .into_iter()
            .flatten()
            .min_by_key(|kept| kept.rank)
            .map(|kept| kept.value)
    }

    /// Keeps `key` with `value`; an earlier kept location of the same place stays first.
    pub(super) fn insert(&mut self, key: PlaceKey<'_>, value: T) {
        let kept = Kept {
            rank: self.len,
            value,
        };
        self.len += 1;
        let location = key.location;
        let provider = location.provider().clone();
        if location.folds_case() {
            let folded = key.folded.unwrap_or_else(|| fold_key(location));
            let keys = self.folded.entry(provider).or_default();
            keys.entry(folded).or_insert(kept);
        } else {
            let paths = self.exact.entry(provider).or_default();
            paths.entry(location.path().clone()).or_insert(kept);
        }
    }
}
