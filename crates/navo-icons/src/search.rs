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
    names_buf: String,
    name_offsets: Vec<u32>,
    tags_buf: String,
    tag_offsets: Vec<u32>,
}

impl SetEntries {
    fn len(&self) -> usize {
        self.name_offsets.len().saturating_sub(1)
    }

    fn name(&self, idx: usize) -> &str {
        slice_at(&self.names_buf, &self.name_offsets, idx)
    }

    fn tags(&self, idx: usize) -> &str {
        slice_at(&self.tags_buf, &self.tag_offsets, idx)
    }
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
            let expected = usize::try_from(info.count).unwrap_or(0);
            let (names_buf, name_offsets) = pack_names(registry, &info.id, expected);
            let count = name_offsets.len().saturating_sub(1);
            let downloaded = registry.is_downloaded(&info.id);
            let raw_tags = registry.tags(&info.id).unwrap_or_default();
            let (tags_buf, tag_offsets) = pack_tags(&raw_tags, count);
            sets.push(SetEntries {
                id: info.id,
                downloaded,
                names_buf,
                name_offsets,
                tags_buf,
                tag_offsets,
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
            for idx in 0..set.len().min(cap) {
                out.push(Hit {
                    set: set.id.clone(),
                    name: set.name(idx).to_owned(),
                });
            }
            continue;
        }
        collect_set_hits(set, &full_query, &words, cap, &mut out);
    }
    out
}

fn pack_names(registry: &Registry, set_id: &str, expected: usize) -> (String, Vec<u32>) {
    let mut names_buf = String::with_capacity(expected.saturating_mul(16));
    let mut name_offsets = Vec::with_capacity(expected.saturating_add(1));
    name_offsets.push(0);
    registry.for_each_name(set_id, |name| {
        if let Ok(end) = u32::try_from(names_buf.len().saturating_add(name.len())) {
            names_buf.push_str(name);
            name_offsets.push(end);
        }
    });
    names_buf.shrink_to_fit();
    name_offsets.shrink_to_fit();
    (names_buf, name_offsets)
}

fn pack_tags(raw_tags: &str, count: usize) -> (String, Vec<u32>) {
    let lowered = to_lower_cow(raw_tags);
    let mut tags_buf = String::with_capacity(lowered.len());
    let mut tag_offsets = Vec::with_capacity(count.saturating_add(1));
    tag_offsets.push(0);
    for line in lowered.lines().take(count) {
        if let Ok(end) = u32::try_from(tags_buf.len().saturating_add(line.len())) {
            tags_buf.push_str(line);
            tag_offsets.push(end);
        } else {
            tag_offsets.push(*tag_offsets.last().unwrap_or(&0));
        }
    }
    let tail = *tag_offsets.last().unwrap_or(&0);
    while tag_offsets.len() <= count {
        tag_offsets.push(tail);
    }
    tags_buf.shrink_to_fit();
    tag_offsets.shrink_to_fit();
    (tags_buf, tag_offsets)
}

fn slice_at<'a>(buf: &'a str, offsets: &[u32], idx: usize) -> &'a str {
    let (Some(&start), Some(&end)) = (offsets.get(idx), offsets.get(idx + 1)) else {
        return "";
    };
    buf.get(start as usize..end as usize).unwrap_or("")
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
    for idx in 0..set.len() {
        let name = set.name(idx);
        let tags = set.tags(idx);
        let lower_name = to_lower_cow(name);
        if let Some(rank) = rank_icon(&lower_name, tags, full_query, words) {
            ranked.push((rank, name));
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
