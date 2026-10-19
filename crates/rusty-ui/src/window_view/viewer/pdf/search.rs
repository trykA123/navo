use std::collections::BTreeMap;

use crate::format;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Query {
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
}

impl Query {
    pub(super) fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Hit {
    pub page: u32,
    pub start: u32,
    pub len: u32,
    pub y: f32,
}

impl Hit {
    fn key(&self) -> (u32, u32) {
        (self.page, self.start)
    }
}

pub(super) struct Snippet {
    pub before: String,
    pub hit: String,
    pub after: String,
}

const CONTEXT: usize = 36;

fn fold(ch: char, match_case: bool) -> char {
    if match_case {
        return ch;
    }
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => ch,
    }
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn find(chars: &[char], query: &Query) -> Vec<(u32, u32)> {
    let needle: Vec<char> = query
        .text
        .chars()
        .map(|ch| fold(ch, query.match_case))
        .collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut at = 0;
    while at + needle.len() <= chars.len() {
        let fits = chars[at..at + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(have, want)| fold(*have, query.match_case) == *want);
        let end = at + needle.len();
        let bounded = !query.whole_word
            || ((at == 0 || !is_word(chars[at - 1]))
                && chars.get(end).is_none_or(|ch| !is_word(*ch)));
        if fits && bounded {
            found.push((at as u32, needle.len() as u32));
            at = end;
        } else {
            at += 1;
        }
    }
    found
}

fn flat(chars: &[char]) -> String {
    let mut text = String::with_capacity(chars.len());
    for ch in chars {
        let ch = if ch.is_whitespace() { ' ' } else { *ch };
        if !(ch == ' ' && text.ends_with(' ')) {
            text.push(ch);
        }
    }
    text
}

pub(super) fn snippet(chars: &[char], hit: &Hit) -> Snippet {
    let start = (hit.start as usize).min(chars.len());
    let end = (start + hit.len as usize).min(chars.len());
    let from = start.saturating_sub(CONTEXT);
    let to = (end + CONTEXT).min(chars.len());
    let mut before = flat(&chars[from..start]).trim_start().to_owned();
    let mut after = flat(&chars[end..to]).trim_end().to_owned();
    if from > 0 {
        before.insert(0, '…');
    }
    if to < chars.len() {
        after.push('…');
    }
    Snippet {
        before,
        hit: flat(&chars[start..end]),
        after,
    }
}

#[derive(Default)]
pub(super) struct Hits {
    by_page: BTreeMap<u32, Vec<Hit>>,
    total: usize,
    current: Option<(u32, u32)>,
}

impl Hits {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn add_page(&mut self, page: u32, hits: Vec<Hit>) {
        if hits.is_empty() {
            return;
        }
        let old = self.by_page.insert(page, hits);
        self.total = self.total - old.map_or(0, |old| old.len()) + self.on(page).len();
    }

    pub(super) fn total(&self) -> usize {
        self.total
    }

    pub(super) fn on(&self, page: u32) -> &[Hit] {
        self.by_page.get(&page).map_or(&[], Vec::as_slice)
    }

    pub(super) fn pages(&self) -> impl Iterator<Item = (u32, &[Hit])> {
        self.by_page
            .iter()
            .map(|(page, hits)| (*page, hits.as_slice()))
    }

    pub(super) fn all(&self) -> impl DoubleEndedIterator<Item = &Hit> {
        self.by_page.values().flatten()
    }

    pub(super) fn current(&self) -> Option<Hit> {
        let (page, start) = self.current?;
        self.on(page).iter().find(|hit| hit.start == start).copied()
    }

    pub(super) fn set_current(&mut self, hit: Option<Hit>) {
        self.current = hit.map(|hit| hit.key());
    }

    pub(super) fn ordinal(&self) -> Option<usize> {
        let (page, start) = self.current?;
        let before: usize = self.by_page.range(..page).map(|(_, hits)| hits.len()).sum();
        let at = self.on(page).iter().position(|hit| hit.start == start)?;
        Some(before + at + 1)
    }

    pub(super) fn step(&self, from: (u32, u32), backward: bool) -> Option<Hit> {
        if backward {
            self.all().rev().find(|hit| hit.key() < from).copied()
        } else {
            self.all().find(|hit| hit.key() > from).copied()
        }
    }

    pub(super) fn wrap(&self, backward: bool) -> Option<Hit> {
        if backward {
            self.all().next_back().copied()
        } else {
            self.all().next().copied()
        }
    }
}

pub(super) fn count_label(hits: &Hits, is_complete: bool) -> String {
    let total = format::count(hits.total() as u64, "hit", "hits");
    let so_far = if is_complete { "" } else { " so far" };
    match (hits.total(), hits.ordinal()) {
        (0, _) if is_complete => "No hits".to_owned(),
        (0, _) => "No hits so far".to_owned(),
        (count, Some(place)) => format!(
            "{} of {}{so_far}",
            format::bytes(place as u64),
            format::bytes(count as u64)
        ),
        (_, None) => format!("{total}{so_far}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn query(text: &str, match_case: bool, whole_word: bool) -> Query {
        Query {
            text: text.to_owned(),
            match_case,
            whole_word,
        }
    }

    fn hit(page: u32, start: u32) -> Hit {
        Hit {
            page,
            start,
            len: 2,
            y: 0.,
        }
    }

    #[test]
    fn find_ignores_case_unless_asked() {
        let page = chars("Volt volt VOLT");
        assert_eq!(find(&page, &query("volt", false, false)).len(), 3);
        assert_eq!(find(&page, &query("volt", true, false)), [(5, 4)]);
    }

    #[test]
    fn whole_word_skips_hits_inside_longer_words() {
        let page = chars("undervoltage volt volts");
        assert_eq!(find(&page, &query("volt", false, true)), [(13, 4)]);
    }

    #[test]
    fn a_snippet_keeps_context_on_one_line() {
        let page = chars("first line\r\nsecond   word here");
        let shown = snippet(
            &page,
            &Hit {
                page: 0,
                start: 21,
                len: 4,
                y: 0.,
            },
        );
        assert_eq!(
            (
                shown.before.as_str(),
                shown.hit.as_str(),
                shown.after.as_str()
            ),
            ("first line second ", "word", " here")
        );
    }

    #[test]
    fn stepping_moves_through_pages_in_order_and_counts_its_place() {
        let mut hits = Hits::default();
        hits.add_page(4, vec![hit(4, 1)]);
        hits.add_page(1, vec![hit(1, 3), hit(1, 9)]);
        let next = hits.step((1, 3), false);
        assert_eq!(next.map(|hit| (hit.page, hit.start)), Some((1, 9)));
        hits.set_current(hits.step((1, 9), false));
        assert_eq!(hits.ordinal(), Some(3));
        assert_eq!(count_label(&hits, false), "3 of 3 so far");
        assert_eq!(hits.step((4, 1), false), None);
        assert_eq!(hits.wrap(false).map(|hit| hit.page), Some(1));
    }

    #[test]
    fn the_label_says_when_nothing_was_found() {
        let hits = Hits::default();
        assert_eq!(count_label(&hits, true), "No hits");
        assert_eq!(count_label(&hits, false), "No hits so far");
    }
}
