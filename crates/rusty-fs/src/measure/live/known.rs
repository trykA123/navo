use std::collections::{HashMap, VecDeque};

use super::super::largest::renamed;

pub(super) const KNOWN_LIMIT: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Known {
    File(u64),
    Folder,
    Gone,
}

#[derive(Debug, Default)]
pub(super) struct KnownPaths {
    paths: HashMap<String, (Known, u64)>,
    order: VecDeque<(String, u64)>,
    stamp: u64,
}

impl KnownPaths {
    pub(super) fn get(&self, path: &str) -> Option<Known> {
        self.paths.get(path).map(|(known, _)| *known)
    }

    pub(super) fn room(&self) -> usize {
        KNOWN_LIMIT.saturating_sub(self.paths.len())
    }

    pub(super) fn insert(&mut self, path: String, known: Known) {
        self.stamp += 1;
        self.order.push_back((path.clone(), self.stamp));
        self.paths.insert(path, (known, self.stamp));
        while self.paths.len() > KNOWN_LIMIT {
            let Some((oldest, stamp)) = self.order.pop_front() else {
                break;
            };
            if self.paths.get(&oldest).is_some_and(|(_, at)| *at == stamp) {
                self.paths.remove(&oldest);
            }
        }
        if self.order.len() > 2 * KNOWN_LIMIT {
            let paths = &self.paths;
            self.order
                .retain(|(path, stamp)| paths.get(path).is_some_and(|(_, at)| at == stamp));
        }
    }

    pub(super) fn remove(&mut self, path: &str) -> Option<Known> {
        self.paths.remove(path).map(|(known, _)| known)
    }

    pub(super) fn take_below(&mut self, folder: &str, separator: char) -> Vec<(String, Known)> {
        let below: Vec<String> = self
            .paths
            .keys()
            .filter(|path| is_below(path, folder, separator))
            .cloned()
            .collect();
        below
            .into_iter()
            .filter_map(|path| self.remove(&path).map(|known| (path, known)))
            .collect()
    }

    pub(super) fn rename(&mut self, from: &str, to: &str, separator: char) {
        let moved: Vec<(String, Known)> = self
            .paths
            .keys()
            .filter(|path| *path == from || is_below(path, from, separator))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|path| self.remove(&path).map(|known| (path, known)))
            .collect();
        for (path, known) in moved {
            if let Some(path) = renamed(&path, from, to, separator) {
                self.insert(path, known);
            }
        }
    }
}

fn is_below(path: &str, folder: &str, separator: char) -> bool {
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with(separator))
}
