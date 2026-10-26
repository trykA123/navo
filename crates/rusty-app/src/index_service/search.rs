use std::collections::HashMap;

use rusty_index::pipe::{self, CancelToken, Hit, Request};
use rusty_index::table::NameTable;
use rusty_store::Search;

const MAX_DEPTH: usize = 4096;

pub(super) struct SearchIndex {
    config: Search,
    names: Vec<String>,
    paths: Vec<String>,
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new(&Search::default())
    }
}

impl SearchIndex {
    pub fn new(config: &Search) -> Self {
        let (paths, names): (Vec<_>, Vec<_>) = config
            .exclude
            .iter()
            .cloned()
            .partition(|skip| skip.contains(['\\', '/']));
        Self {
            config: config.clone(),
            names,
            paths: paths.iter().map(|path| normalized_path(path)).collect(),
        }
    }

    pub fn hits(&self, table: &NameTable, request: &Request, cancel: &CancelToken) -> Vec<Hit> {
        rusty_index::search::search_filtered_with(
            table,
            &request.terms(),
            rusty_index::search::SearchOptions {
                limit: pipe::MAX_HITS,
                cancel: &|| cancel.is_cancelled(),
                include: &|| {
                    let mut eligibility = Eligibility::new(table, self);
                    move |id| eligibility.includes(id)
                },
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
    index: &'a SearchIndex,
    folders: HashMap<u32, bool>,
    #[cfg(test)]
    evaluated: usize,
}

impl<'a> Eligibility<'a> {
    fn new(table: &'a NameTable, index: &'a SearchIndex) -> Self {
        Self {
            table,
            index,
            folders: HashMap::new(),
            #[cfg(test)]
            evaluated: 0,
        }
    }

    fn includes(&mut self, id: u32) -> bool {
        if self.self_excluded(id) {
            return false;
        }
        match self.table.parent(id) {
            Some(parent) => self.folder_included(parent),
            None => selected_root(self.table, id, &self.index.config.drives),
        }
    }

    fn folder_included(&mut self, start: u32) -> bool {
        let mut chain = Vec::new();
        let mut current = start;
        let keep = loop {
            if let Some(&known) = self.folders.get(&current) {
                break known;
            }
            chain.push(current);
            if chain.len() >= MAX_DEPTH || self.self_excluded(current) {
                break false;
            }
            #[cfg(test)]
            {
                self.evaluated += 1;
            }
            match self.table.parent(current) {
                Some(parent) => current = parent,
                None => break selected_root(self.table, current, &self.index.config.drives),
            }
        };
        for id in chain {
            self.folders.insert(id, keep);
        }
        keep
    }

    fn self_excluded(&self, id: u32) -> bool {
        self.table
            .name(id)
            .is_none_or(|name| self.index.excluded(self.table, id, name))
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

impl SearchIndex {
    fn excluded(&self, table: &NameTable, id: u32, name: &str) -> bool {
        if self
            .names
            .iter()
            .any(|skip| name.eq_ignore_ascii_case(skip))
        {
            return true;
        }
        !self.paths.is_empty()
            && table.path_with(id, '\\').is_some_and(|path| {
                self.paths
                    .iter()
                    .any(|skip| path.eq_ignore_ascii_case(skip))
            })
    }
}

#[cfg(test)]
static NORMALIZATIONS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, usize>>> =
    std::sync::LazyLock::new(Default::default);

fn normalized_path(path: &str) -> String {
    #[cfg(test)]
    {
        let mut counts = NORMALIZATIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *counts.entry(path.to_owned()).or_default() += 1;
    }
    path.replace('/', "\\").trim_end_matches('\\').to_owned()
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
        let index = SearchIndex::new(&config);
        let mut eligibility = Eligibility::new(&table, &index);
        assert!(matches.iter().all(|&id| eligibility.includes(id)));
        assert_eq!(eligibility.evaluated, 2);
    }

    #[test]
    fn a_million_candidates_normalize_each_path_exclusion_only_once() {
        let exclusion = "c:/million-candidate-exclusion/";
        NORMALIZATIONS.lock().unwrap().remove(exclusion);
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let folder = table.push(root, "work", Flags::FOLDER).unwrap();
        for _ in 0..1_000_000 {
            table.push(folder, "match", Flags::NONE).unwrap();
        }
        let index = SearchIndex::new(&Search {
            exclude: vec![exclusion.into()],
            ..Search::default()
        });
        let request = Request {
            terms: vec![QueryTerm::Name("match".into())],
        };
        assert_eq!(
            index.hits(&table, &request, &CancelToken::new()).len(),
            pipe::MAX_HITS
        );
        assert_eq!(NORMALIZATIONS.lock().unwrap().get(exclusion), Some(&1));
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

#[cfg(test)]
mod default_tests {
    use super::*;
    use rusty_index::{
        pipe::QueryTerm,
        table::{Flags, NO_PARENT},
    };
    #[test]
    fn default_construction_keeps_the_default_folder_exclusions() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let skipped = table.push(root, "$Recycle.Bin", Flags::FOLDER).unwrap();
        table.push(skipped, "match", Flags::NONE).unwrap();
        let query = Request {
            terms: vec![QueryTerm::Name("match".into())],
        };
        assert!(
            SearchIndex::default()
                .hits(&table, &query, &CancelToken::new())
                .is_empty()
        );
    }
}
