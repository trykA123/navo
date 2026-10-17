use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

pub const LARGEST_LIMIT: usize = 10;
pub const RANKING_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFile {
    pub path: String,
    pub bytes: u64,
}

impl Ord for LargeFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes
            .cmp(&other.bytes)
            .then_with(|| other.path.cmp(&self.path))
    }
}

impl PartialOrd for LargeFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ranking {
    files: Vec<LargeFile>,
    floor: Option<u64>,
}

impl Ranking {
    #[must_use]
    pub fn files(&self) -> &[LargeFile] {
        todo!()
    }

    #[must_use]
    pub fn is_short(&self) -> bool {
        todo!()
    }

    pub(super) fn top(&self) -> Vec<LargeFile> {
        todo!()
    }

    pub(super) fn size_of(&self, _path: &str) -> Option<u64> {
        todo!()
    }

    pub(super) fn offer(&mut self, _file: LargeFile) {
        todo!()
    }

    pub(super) fn remove(&mut self, _path: &str) -> Option<u64> {
        todo!()
    }

    pub(super) fn rename(&mut self, _from: &str, _to: &str, _separator: char) {
        todo!()
    }
}

#[derive(Debug, Default)]
pub(super) struct Largest {
    heap: BinaryHeap<Reverse<LargeFile>>,
}

impl Largest {
    pub(super) fn admits(&self, bytes: u64) -> bool {
        self.heap.len() < LARGEST_LIMIT
            || self
                .heap
                .peek()
                .is_some_and(|Reverse(least)| bytes >= least.bytes)
    }

    pub(super) fn offer(&mut self, file: LargeFile) {
        self.heap.push(Reverse(file));
        if self.heap.len() > LARGEST_LIMIT {
            self.heap.pop();
        }
    }

    pub(super) fn snapshot(&self) -> Vec<LargeFile> {
        let mut files: Vec<LargeFile> = self.heap.iter().map(|Reverse(f)| f.clone()).collect();
        files.sort_by(|a, b| b.cmp(a));
        files
    }
}
