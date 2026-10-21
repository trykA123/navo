use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use rusty_index::pipe::{self, CancelToken, Hit, Request};
use rusty_index::table::NameTable;
use rusty_store::Search;

const MAX_DEPTH: usize = 4096;

#[derive(Default)]
pub(super) struct SearchIndex {
    config: Search,
}

impl SearchIndex {
    pub fn new(config: &Search) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub fn hits(&self, table: &NameTable, request: &Request, cancel: &CancelToken) -> Vec<Hit> {
        let eligibility = Eligibility::new(table, &self.config);
        rusty_index::search::search_filtered(
            table,
            &request.terms(),
            rusty_index::search::SearchOptions {
                limit: pipe::MAX_HITS,
                cancel: &|| cancel.is_cancelled(),
                include: &|id| eligibility.includes(id),
            },
        )
        .into_iter()
        .filter_map(|hit| {
            Some(Hit {
                path: table.path_with(hit.id, '\\')?,
                flags: table.flags(hit.id)?.bits(),
                score: hit.score,
                indices: hit.indices,
            })
        })
        .collect()
    }
}

struct Eligibility<'a> {
    table: &'a NameTable,
    config: &'a Search,
    folders: Mutex<HashMap<u32, bool>>,
    evaluated: AtomicUsize,
}

impl<'a> Eligibility<'a> {
    fn new(table: &'a NameTable, config: &'a Search) -> Self {
        Self {
            table,
            config,
            folders: Mutex::new(HashMap::new()),
            evaluated: AtomicUsize::new(0),
        }
    }

    fn includes(&self, id: u32) -> bool {
        if self.self_excluded(id) {
            return false;
        }
        match self.table.parent(id) {
            Some(parent) => self.folder_included(parent),
            None => selected_root(self.table, id, &self.config.drives),
        }
    }

    fn folder_included(&self, start: u32) -> bool {
        let mut chain = Vec::new();
        let mut current = start;
        let keep = loop {
            if let Some(&known) = self.memo().get(&current) {
                break known;
            }
            chain.push(current);
            if chain.len() >= MAX_DEPTH || self.self_excluded(current) {
                break false;
            }
            self.evaluated.fetch_add(1, Ordering::Relaxed);
            match self.table.parent(current) {
                Some(parent) => current = parent,
                None => break selected_root(self.table, current, &self.config.drives),
            }
        };
        let mut memo = self.memo();
        for id in chain {
            memo.insert(id, keep);
        }
        keep
    }

    fn self_excluded(&self, id: u32) -> bool {
        self.table
            .name(id)
            .is_none_or(|name| excluded(self.table, id, name, &self.config.exclude))
    }

    fn memo(&self) -> std::sync::MutexGuard<'_, HashMap<u32, bool>> {
        self.folders.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn selected_root(table: &NameTable, id: u32, drives: &[String]) -> bool {
    drives.is_empty()
        || drives.iter().any(|drive| {
            table
                .name(id)
                .is_some_and(|root| root.eq_ignore_ascii_case(drive.trim_end_matches(['\\', '/'])))
        })
}

fn excluded(table: &NameTable, id: u32, name: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|skip| {
        name.eq_ignore_ascii_case(skip)
            || (skip.contains(['\\', '/'])
                && table.path_with(id, '\\').is_some_and(|path| {
                    path.eq_ignore_ascii_case(skip.replace('/', "\\").trim_end_matches('\\'))
                }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_index::pipe::QueryTerm;
    use rusty_index::table::{Flags, NO_PARENT};

    #[test]
    fn excluded_matches_and_unselected_drives_never_take_the_top_five_hundred() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let skip = table.push(root, "skip", Flags::FOLDER).unwrap();
        let other = table.push(NO_PARENT, "D:", Flags::FOLDER).unwrap();
        for parent in [skip, other] {
            for _ in 0..600 {
                table.push(parent, "match", Flags::NONE).unwrap();
            }
        }
        for name in ["match-allowed-a", "match-allowed-b", "match-allowed-c"] {
            table.push(root, name, Flags::NONE).unwrap();
        }
        let request = Request {
            terms: vec![QueryTerm::Name("match".into())],
        };
        for exclusion in ["skip", "c:/skip/"] {
            let config = Search {
                drives: vec!["C:\\".into()],
                exclude: vec![exclusion.into()],
                ..Search::default()
            };
            let selected = SearchIndex::new(&config);
            let hits = selected.hits(&table, &request, &CancelToken::new());
            assert_eq!(hits.len(), 3);
            assert!(
                hits.iter()
                    .all(|hit| hit.path.starts_with("C:\\match-allowed"))
            );
        }
    }

    #[test]
    fn a_query_checks_each_ancestor_once_and_never_scans_every_slot() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let folder = table.push(root, "work", Flags::FOLDER).unwrap();
        for _ in 0..5_000 {
            table.push(folder, "noise", Flags::NONE).unwrap();
        }
        let matches: Vec<u32> = (0..300)
            .map(|_| table.push(folder, "match", Flags::NONE).unwrap())
            .collect();
        let config = Search {
            exclude: vec!["skip".into()],
            ..Search::default()
        };
        let eligibility = Eligibility::new(&table, &config);
        assert!(matches.iter().all(|&id| eligibility.includes(id)));
        assert_eq!(eligibility.evaluated.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn moving_a_folder_rebuilds_its_descendants_eligibility() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let folder = table.push(root, "allowed", Flags::FOLDER).unwrap();
        table.push(folder, "match", Flags::NONE).unwrap();
        let config = Search {
            exclude: vec!["skip".into()],
            ..Search::default()
        };
        let request = Request {
            terms: vec![QueryTerm::Name("match".into())],
        };
        assert_eq!(
            SearchIndex::new(&config)
                .hits(&table, &request, &CancelToken::new())
                .len(),
            1
        );
        table.rename(folder, root, "skip").unwrap();
        assert!(
            SearchIndex::new(&config)
                .hits(&table, &request, &CancelToken::new())
                .is_empty()
        );
    }
}
