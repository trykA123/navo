//! In-memory tag index and ranked search across bundled and downloaded icon sets.

use std::borrow::Cow;

use crate::registry::Registry;

/// Which icon sets a search query should inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetFilter {
    /// Search every bundled and downloaded set, capped per set.
    All,
    /// Search only the named set without applying a per-set cap.
    Set(String),
    /// Search only downloaded sets, capped per set.
    Downloaded,
}

/// One matching icon returned by [`search`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The set id containing the icon.
    pub set: String,
    /// The icon name within its set.
    pub name: String,
}

struct SetEntries {
    id: String,
    downloaded: bool,
    names: Vec<String>,
    tags: Vec<String>,
}

/// Preloaded icon names and lower-cased tag lines for every set in a [`Registry`].
pub struct TagIndex {
    sets: Vec<SetEntries>,
}

impl TagIndex {
    /// Loads names and tag lines for every set currently registered in `registry`.
    pub fn load(registry: &Registry) -> Self {
        let mut sets = Vec::new();
        for info in registry.sets() {
            let names = registry.names(&info.id);
            let downloaded = registry.is_downloaded(&info.id);
            let raw_tags = registry.tags(&info.id).unwrap_or_default();
            let lowered = if is_lower_ascii(&raw_tags) {
                raw_tags
            } else {
                raw_tags.to_lowercase()
            };
            let mut tags: Vec<String> = lowered
                .lines()
                .take(names.len())
                .map(str::to_owned)
                .collect();
            if tags.len() < names.len() {
                tags.resize_with(names.len(), String::new);
            }
            sets.push(SetEntries {
                id: info.id,
                downloaded,
                names,
                tags,
            });
        }
        Self { sets }
    }

    /// Returns the number of icon sets indexed.
    pub fn set_count(&self) -> usize {
        self.sets.len()
    }
}

/// Searches `index` for icons matching `query` under `filter`, returning at most `per_set` hits
/// per set (ignored for [`SetFilter::Set`]).
pub fn search(index: &TagIndex, query: &str, filter: &SetFilter, per_set: usize) -> Vec<Hit> {
    let full_query = query.trim().to_lowercase();
    let words: Vec<&str> = full_query.split_ascii_whitespace().collect();
    let cap = match filter {
        SetFilter::Set(_) => usize::MAX,
        SetFilter::All | SetFilter::Downloaded => per_set,
    };
    if cap == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    for set in &index.sets {
        if !matches_filter(set, filter) {
            continue;
        }
        if words.is_empty() {
            for name in set.names.iter().take(cap) {
                out.push(Hit {
                    set: set.id.clone(),
                    name: name.clone(),
                });
            }
            continue;
        }
        collect_set_hits(set, &full_query, &words, cap, &mut out);
    }
    out
}

fn matches_filter(set: &SetEntries, filter: &SetFilter) -> bool {
    match filter {
        SetFilter::All => true,
        SetFilter::Set(id) => set.id == *id,
        SetFilter::Downloaded => set.downloaded,
    }
}

fn collect_set_hits(
    set: &SetEntries,
    full_query: &str,
    words: &[&str],
    cap: usize,
    out: &mut Vec<Hit>,
) {
    let mut ranked: Vec<(u8, &str)> = Vec::new();
    for (name, tags) in set.names.iter().zip(set.tags.iter()) {
        let lower_name = to_lower_cow(name);
        if let Some(rank) = rank_icon(&lower_name, tags, full_query, words) {
            ranked.push((rank, name.as_str()));
        }
    }
    ranked.sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));
    for (_, name) in ranked.into_iter().take(cap) {
        out.push(Hit {
            set: set.id.clone(),
            name: name.to_owned(),
        });
    }
}

/// Ranks a matching icon from 0 (exact name) to 4 (tag contains / mixed); `None` if any word misses.
fn rank_icon(name: &str, tags: &str, full_query: &str, words: &[&str]) -> Option<u8> {
    if !words
        .iter()
        .all(|word| name.contains(word) || tags.contains(word))
    {
        return None;
    }
    if name == full_query {
        return Some(0);
    }
    if name.starts_with(full_query) || words.iter().all(|word| name.starts_with(word)) {
        return Some(1);
    }
    if words.iter().all(|word| name.contains(word)) {
        return Some(2);
    }
    if words
        .iter()
        .all(|word| name.contains(word) || has_tag_prefix(tags, word))
    {
        return Some(3);
    }
    Some(4)
}

fn has_tag_prefix(tags: &str, word: &str) -> bool {
    tags.split_ascii_whitespace()
        .any(|token| token.starts_with(word))
}

fn is_lower_ascii(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii() && !byte.is_ascii_uppercase())
}

fn to_lower_cow(text: &str) -> Cow<'_, str> {
    if is_lower_ascii(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_lowercase())
    }
}

#[cfg(test)]
mod tests;
