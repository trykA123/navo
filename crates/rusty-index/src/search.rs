use std::cmp::Reverse;
use std::collections::BinaryHeap;

use nucleo_matcher::{Utf32Str, chars::graphemes};
use rayon::prelude::*;

use crate::fuzzy::{Fuzzy, Match};
use crate::table::{Flags, NameTable};

const CHUNK_SIZE: u32 = 64 * 1024;
type Candidates = BinaryHeap<(Reverse<u32>, u32, Vec<u32>)>;

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

pub fn search(
    table: &NameTable,
    query: &[Term<'_>],
    limit: usize,
    cancel: &(impl Fn() -> bool + Sync),
) -> Vec<TableHit> {
    if limit == 0 || table.is_empty() || cancel() {
        return Vec::new();
    }
    let request = Search {
        table,
        limit,
        cancel,
    };
    let hits = (0..table.slots().div_ceil(CHUNK_SIZE))
        .into_par_iter()
        .map_init(
            || Matchers::new(query),
            |matchers, chunk| request.scan_chunk(matchers, chunk),
        )
        .reduce(Candidates::new, |left, right| request.merge(left, right));
    if cancel() {
        return Vec::new();
    }
    hits.into_sorted_vec()
        .into_iter()
        .map(|(Reverse(score), id, indices)| TableHit { id, score, indices })
        .collect()
}

struct Search<'a, C> {
    table: &'a NameTable,
    limit: usize,
    cancel: &'a C,
}

impl<C: Fn() -> bool + Sync> Search<'_, C> {
    fn scan_chunk(&self, matchers: &mut Matchers<'_>, chunk: u32) -> Candidates {
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
            if let Some(hit) = matchers.score(self.table, id, name) {
                keep_hit(&mut hits, (Reverse(hit.score), id, hit.indices), self.limit);
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

fn keep_hit(hits: &mut Candidates, hit: (Reverse<u32>, u32, Vec<u32>), limit: usize) {
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
                Term::Path(text) => matchers.paths.push(Fuzzy::new(
                    &text.replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR),
                    true,
                )),
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
            let path = table.path(id)?;
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

fn name_offset(path: &str, name: &str) -> u32 {
    let mut buffer = Vec::new();
    let text = Utf32Str::new(path, &mut buffer);
    let name_len = match text {
        Utf32Str::Ascii(_) => name.len(),
        Utf32Str::Unicode(_) => graphemes(name).count(),
    };
    (text.len() - name_len) as u32
}

#[cfg(test)]
mod tests;
