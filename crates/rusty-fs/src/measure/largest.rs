use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

pub const LARGEST_LIMIT: usize = 10;

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
