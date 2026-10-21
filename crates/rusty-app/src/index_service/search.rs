use rusty_index::pipe::{self, CancelToken, Hit, Request};
use rusty_index::table::NameTable;
use rusty_store::Search;

#[derive(Default)]
pub(super) struct SearchIndex {
    included: Vec<bool>,
}

impl SearchIndex {
    pub fn new(table: &NameTable, config: &Search) -> Self {
        let mut included = vec![None; table.slots() as usize];
        for (id, _) in table.iter() {
            select_entry(table, id, config, &mut included);
        }
        Self {
            included: included
                .into_iter()
                .map(|keep| keep == Some(true))
                .collect(),
        }
    }

    pub fn hits(&self, table: &NameTable, request: &Request, cancel: &CancelToken) -> Vec<Hit> {
        rusty_index::search::search_filtered(
            table,
            &request.terms(),
            rusty_index::search::SearchOptions {
                limit: pipe::MAX_HITS,
                cancel: &|| cancel.is_cancelled(),
                include: &|id| self.included.get(id as usize) == Some(&true),
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

fn select_entry(table: &NameTable, id: u32, config: &Search, included: &mut [Option<bool>]) {
    let mut chain = Vec::new();
    let mut current = id;
    let keep = loop {
        if let Some(keep) = included[current as usize] {
            break keep;
        }
        if chain.contains(&current)
            || table
                .name(current)
                .is_none_or(|name| excluded(table, current, name, &config.exclude))
        {
            break false;
        }
        chain.push(current);
        let Some(parent) = table.parent(current) else {
            break selected_root(table, current, &config.drives);
        };
        current = parent;
    };
    included[current as usize] = Some(keep);
    for id in chain {
        included[id as usize] = Some(keep);
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
            let selected = SearchIndex::new(&table, &config);
            let hits = selected.hits(&table, &request, &CancelToken::new());
            assert_eq!(hits.len(), 3);
            assert!(
                hits.iter()
                    .all(|hit| hit.path.starts_with("C:\\match-allowed"))
            );
        }
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
            SearchIndex::new(&table, &config)
                .hits(&table, &request, &CancelToken::new())
                .len(),
            1
        );
        table.rename(folder, root, "skip").unwrap();
        assert!(
            SearchIndex::new(&table, &config)
                .hits(&table, &request, &CancelToken::new())
                .is_empty()
        );
    }
}
