//! Replay storage for the legacy NTFS FakeVolume fixture.
use super::*;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub(super) struct ReplayBuffers(Vec<Option<Vec<u8>>>, usize);

impl From<Vec<Option<Vec<u8>>>> for ReplayBuffers {
    /// Retains the supplied bounded enumeration responses, including EOF.
    fn from(buffers: Vec<Option<Vec<u8>>>) -> Self {
        Self(buffers, 0)
    }
}

impl ReplayBuffers {
    /// Restarts at zero; otherwise requires the previous response's header cursor.
    pub(super) fn read(&mut self, cursor: u64) -> Option<Vec<u8>> {
        if cursor == 0 {
            self.1 = 0;
        } else {
            let previous = self.0[self.1 - 1].as_ref().unwrap();
            assert_eq!(previous.get(..8), Some(cursor.to_le_bytes().as_slice()));
        }
        let response = self.0.get(self.1).cloned().flatten();
        self.1 += 1;
        response
    }
}

#[derive(Debug)]
pub(super) struct ReplayQueries(VecDeque<Vec<u8>>, Option<Vec<u8>>);

impl<T: IntoIterator<Item = Vec<u8>>> From<T> for ReplayQueries {
    /// Retains journal responses supplied by existing array and vector fixtures.
    fn from(queries: T) -> Self {
        Self(queries.into_iter().collect(), None)
    }
}

impl Deref for ReplayQueries {
    type Target = VecDeque<Vec<u8>>;
    /// Exposes the fixture queue for existing indexed journal edits.
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ReplayQueries {
    /// Exposes the fixture queue for existing indexed edits and appended responses.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ReplayQueries {
    /// Repeats the last queued metadata; panics if the fixture supplied none.
    pub(super) fn read(&mut self) -> Vec<u8> {
        if let Some(query) = self.0.pop_front() {
            self.1 = Some(query);
        }
        self.1.clone().expect("fixture supplies journal metadata")
    }
}

#[test]
fn replay_returns_identical_records_header_cursors_and_eofs() {
    let buffers = vec![
        Some(bytes(70, &[record(80, 90, "child")])),
        Some(bytes(200, &[])),
        Some(bytes(300, &[record(90, ROOT, "parent")])),
        None,
    ];
    let mut volume = FakeVolume::new(buffers.clone());
    for _ in 0..2 {
        for (cursor, expected) in [0, 70, 200, 300].into_iter().zip(&buffers) {
            assert_eq!(volume.enumerate(cursor).ok().as_ref(), Some(expected));
        }
    }
    for next in [100, 120, 120] {
        assert_eq!(volume.query_journal().ok(), Some(journal(7, 10, next, 1)));
    }
    volume.queries.push_back(journal(7, 10, 140, 1));
    assert_eq!(volume.query_journal().ok(), Some(journal(7, 10, 140, 1)));
}
