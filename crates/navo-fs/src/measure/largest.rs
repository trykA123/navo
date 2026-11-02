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
        &self.files
    }

    #[must_use]
    pub fn is_short(&self) -> bool {
        self.floor.is_some() && self.files.len() < LARGEST_LIMIT
    }

    pub(super) fn top(&self) -> Vec<LargeFile> {
        self.files[..LARGEST_LIMIT.min(self.files.len())].to_vec()
    }

    pub(super) fn size_of(&self, path: &str) -> Option<u64> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.bytes)
    }

    pub(super) fn offer(&mut self, file: LargeFile) {
        self.remove(&file.path);
        if self.floor.is_some_and(|floor| file.bytes < floor) {
            return;
        }
        let at = self.files.partition_point(|kept| *kept > file);
        self.files.insert(at, file);
        if self.files.len() > RANKING_LIMIT
            && let Some(cut) = self.files.pop()
        {
            self.floor = Some(self.floor.map_or(cut.bytes, |floor| floor.max(cut.bytes)));
        }
    }

    pub(super) fn remove(&mut self, path: &str) -> Option<u64> {
        let at = self.files.iter().position(|file| file.path == path)?;
        Some(self.files.remove(at).bytes)
    }

    pub(super) fn rename(&mut self, from: &str, to: &str, separator: char) {
        for file in &mut self.files {
            if let Some(path) = renamed(&file.path, from, to, separator) {
                file.path = path;
            }
        }
        self.files.sort_by(|a, b| b.cmp(a));
    }
}

pub(super) fn renamed(path: &str, from: &str, to: &str, separator: char) -> Option<String> {
    if path == from {
        return Some(to.to_owned());
    }
    let rest = path.strip_prefix(from)?.strip_prefix(separator)?;
    Some(format!("{to}{separator}{rest}"))
}

#[derive(Debug, Default)]
pub(super) struct Largest {
    heap: BinaryHeap<Reverse<LargeFile>>,
    floor: Option<u64>,
}

impl Largest {
    pub(super) fn admits(&self, bytes: u64) -> bool {
        self.heap.len() < RANKING_LIMIT
            || self
                .heap
                .peek()
                .is_some_and(|Reverse(least)| bytes >= least.bytes)
    }

    pub(super) fn offer(&mut self, file: LargeFile) {
        self.heap.push(Reverse(file));
        if self.heap.len() > RANKING_LIMIT
            && let Some(Reverse(cut)) = self.heap.pop()
        {
            self.floor = Some(self.floor.map_or(cut.bytes, |floor| floor.max(cut.bytes)));
        }
    }

    pub(super) fn snapshot(&self) -> Ranking {
        let mut files: Vec<LargeFile> = self.heap.iter().map(|Reverse(f)| f.clone()).collect();
        files.sort_by(|a, b| b.cmp(a));
        Ranking {
            files,
            floor: self.floor,
        }
    }
}
