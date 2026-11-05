use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rayon::prelude::*;

use crate::fuzzy::{Fuzzy, Match};
use crate::table::{Flags, NameTable};

const CHUNK_SIZE: u32 = 16 * 1024;
type Candidates = BinaryHeap<(Reverse<u32>, u32)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term<'a> {
    Name(&'a str),
    Path(&'a str),
    Ext(&'a [String]),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableHit {
    pub id: u32,
    pub score: u32,
    pub indices: Vec<u32>,
}

pub struct SearchOptions<'a, C, I> {
    pub limit: usize,
    pub cancel: &'a C,
    pub include: &'a I,
}

pub fn search(
    table: &NameTable,
    query: &[Term<'_>],
    limit: usize,
    cancel: &(impl Fn() -> bool + Sync),
) -> Vec<TableHit> {
    search_filtered(
        table,
        query,
        &SearchOptions {
            limit,
            cancel,
            include: &|_| true,
        },
    )
}

pub fn search_filtered(
    table: &NameTable,
    query: &[Term<'_>],
    options: &SearchOptions<'_, impl Fn() -> bool + Sync, impl Fn(u32) -> bool + Sync>,
) -> Vec<TableHit> {
    search_filtered_with(
        table,
        query,
        &SearchOptions {
            limit: options.limit,
            cancel: options.cancel,
            include: &|| |id| (options.include)(id),
        },
    )
}

pub fn search_filtered_with<I: FnMut(u32) -> bool>(
    table: &NameTable,
    query: &[Term<'_>],
    options: &SearchOptions<'_, impl Fn() -> bool + Sync, impl Fn() -> I + Sync>,
) -> Vec<TableHit> {
    let SearchOptions {
        limit,
        cancel,
        include,
    } = *options;
    if limit == 0 || table.is_empty() || cancel() {
        return Vec::new();
    }
    if query.is_empty() {
        return first_included(table, limit, include());
    }
    let request = Search {
        table,
        limit,
        cancel,
    };
    let hits = (0..table.slots().div_ceil(CHUNK_SIZE))
        .into_par_iter()
        .map_init(
            || (Matchers::new(query), include()),
            |(matchers, filter), chunk| request.scan_chunk(matchers, filter, chunk),
        )
        .reduce(Candidates::new, |left, right| request.merge(left, right));
    if cancel() {
        return Vec::new();
    }
    highlight(table, query, hits.into_sorted_vec())
}

#[expect(
    clippy::expect_used,
    reason = "the scan matched this live id against the same unchanged table"
)]
fn highlight(
    table: &NameTable,
    query: &[Term<'_>],
    hits: Vec<(Reverse<u32>, u32)>,
) -> Vec<TableHit> {
    if hits.is_empty() {
        return Vec::new();
    }
    let mut matchers = Matchers::new(query);
    hits.into_iter()
        .map(|(Reverse(score), id)| {
            let matched = table
                .name(id)
                .and_then(|name| matchers.score(table, id, name))
                .expect("a scanned hit still matches its unchanged table entry");
            TableHit {
                id,
                score,
                indices: matched.indices,
            }
        })
        .collect()
}

fn first_included(
    table: &NameTable,
    limit: usize,
    mut include: impl FnMut(u32) -> bool,
) -> Vec<TableHit> {
    table
        .iter()
        .filter(|&(id, _)| include(id))
        .take(limit)
        .map(|(id, _)| TableHit {
            id,
            score: 0,
            indices: Vec::new(),
        })
        .collect()
}

struct Search<'a, C> {
    table: &'a NameTable,
    limit: usize,
    cancel: &'a C,
}

impl<C: Fn() -> bool + Sync> Search<'_, C> {
    fn scan_chunk(
        &self,
        matchers: &mut Matchers<'_>,
        include: &mut impl FnMut(u32) -> bool,
        chunk: u32,
    ) -> Candidates {
        if (self.cancel)() {
            return Candidates::new();
        }
        let start = chunk * CHUNK_SIZE;
        let end = start.saturating_add(CHUNK_SIZE).min(self.table.slots());
        let mut hits = Candidates::new();
        for (id, name) in self.table.iter_range(start..end) {
            if (self.cancel)() {
                break;
            }
            if let Some(score) = matchers.score_only(self.table, id, name)
                && include(id)
            {
                keep_hit(&mut hits, (Reverse(score), id), self.limit);
            }
        }
        hits
    }

    fn merge(&self, mut left: Candidates, right: Candidates) -> Candidates {
        for hit in right {
            if (self.cancel)() {
                return Candidates::new();
            }
            keep_hit(&mut left, hit, self.limit);
        }
        left
    }
}

fn keep_hit(hits: &mut Candidates, hit: (Reverse<u32>, u32), limit: usize) {
    if hits.len() < limit {
        hits.push(hit);
    } else if let Some(mut worst) = hits.peek_mut()
        && (hit.0, hit.1) < (worst.0, worst.1)
    {
        *worst = hit;
    }
}

struct Matchers<'a> {
    names: Vec<Fuzzy>,
    paths: Vec<Fuzzy>,
    extensions: Vec<&'a [String]>,
}

impl<'a> Matchers<'a> {
    fn new(query: &[Term<'a>]) -> Self {
        let mut matchers = Self {
            names: Vec::new(),
            paths: Vec::new(),
            extensions: Vec::new(),
        };
        for &term in query {
            match term {
                Term::Name(text) => matchers.names.push(Fuzzy::new(text, false)),
                Term::Path(text) => matchers
                    .paths
                    .push(Fuzzy::new(&text.replace('\\', "/"), true)),
                Term::Ext(extensions) => matchers.extensions.push(extensions),
            }
        }
        matchers
    }

    fn score(&mut self, table: &NameTable, id: u32, name: &str) -> Option<Match> {
        if !self.matches_extension(table, id, name) {
            return None;
        }
        let mut hit = score_terms(&mut self.names, name)?;
        if !self.paths.is_empty() {
            let path = table.path_with(id, '/')?;
            let path_hit = score_terms(&mut self.paths, &path)?;
            let offset = name_offset(&path, name);
            hit.score = hit.score.saturating_add(path_hit.score);
            hit.indices.extend(
                path_hit
                    .indices
                    .into_iter()
                    .filter_map(|index| index.checked_sub(offset)),
            );
        }
        hit.indices.sort_unstable();
        hit.indices.dedup();
        Some(hit)
    }

    fn score_only(&mut self, table: &NameTable, id: u32, name: &str) -> Option<u32> {
        if !self.matches_extension(table, id, name) {
            return None;
        }
        let mut score = score_terms_only(&mut self.names, name)?;
        if !self.paths.is_empty() {
            let path = table.path_with(id, '/')?;
            score = score.saturating_add(score_terms_only(&mut self.paths, &path)?);
        }
        Some(score)
    }

    fn matches_extension(&self, table: &NameTable, id: u32, name: &str) -> bool {
        if self.extensions.is_empty() {
            return true;
        }
        if table
            .flags(id)
            .is_some_and(|flags| flags.contains(Flags::FOLDER))
        {
            return false;
        }
        let Some((stem, extension)) = name.rsplit_once('.') else {
            return false;
        };
        !stem.is_empty()
            && !extension.is_empty()
            && self.extensions.iter().all(|extensions| {
                extensions.iter().any(|expected| {
                    extension
                        .chars()
                        .flat_map(char::to_lowercase)
                        .eq(expected.chars().flat_map(char::to_lowercase))
                })
            })
    }
}

fn score_terms(matchers: &mut [Fuzzy], text: &str) -> Option<Match> {
    let mut combined = Match {
        score: 0,
        indices: Vec::new(),
    };
    for matcher in matchers {
        let hit = matcher.score(text)?;
        combined.score = combined.score.saturating_add(hit.score);
        combined.indices.extend(hit.indices);
    }
    Some(combined)
}

fn score_terms_only(matchers: &mut [Fuzzy], text: &str) -> Option<u32> {
    let mut score = 0u32;
    for matcher in matchers {
        score = score.saturating_add(matcher.score_only(text)?);
    }
    Some(score)
}

fn name_offset(path: &str, name: &str) -> u32 {
    (path.chars().count() - name.chars().count()) as u32
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod worker_filter_tests {
    use super::*;
    use crate::table::NO_PARENT;
    #[test]
    fn worker_filters_keep_private_mutable_state_before_the_result_limit() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        for _ in 0..130_000 {
            table.push(root, "match", Flags::NONE).unwrap();
        }
        let created = std::sync::atomic::AtomicUsize::new(0);
        let result = search_filtered_with(
            &table,
            &[Term::Name("match")],
            &SearchOptions {
                limit: 500,
                cancel: &|| false,
                include: &|| {
                    created.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let mut previous = None;
                    move |id| {
                        assert!(previous.is_none_or(|previous| id > previous));
                        previous = Some(id);
                        id % 2 == 0
                    }
                },
            },
        );
        assert_eq!(result.len(), 500);
        assert!(result.iter().all(|hit| hit.id % 2 == 0));
        assert!(created.load(std::sync::atomic::Ordering::Relaxed) >= 1);
    }
}
