use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: u32,
    pub indices: Vec<u32>,
}

pub struct Fuzzy {
    matcher: Matcher,
    atoms: Vec<Atom>,
    empty: bool,
    ascii: Option<Vec<Vec<u8>>>,
    buf: Vec<char>,
}

impl Fuzzy {
    pub fn new(query: &str, paths: bool) -> Self {
        let mut config = Config::DEFAULT;
        if paths {
            config.set_match_paths();
        }
        Self {
            matcher: Matcher::new(config),
            atoms: plain(query),
            empty: query.trim().is_empty(),
            ascii: (!paths && query.is_ascii()).then(|| {
                query
                    .split_whitespace()
                    .map(|term| term.to_ascii_lowercase().into_bytes())
                    .collect()
            }),
            buf: Vec::new(),
        }
    }

    pub fn score(&mut self, haystack: &str) -> Option<Match> {
        if self.empty {
            return Some(Match {
                score: 0,
                indices: Vec::new(),
            });
        }
        let text = utf32(self.ascii.as_deref(), &mut self.buf, haystack)?;
        #[cfg(test)]
        test_counters::count_indices_computed();
        let mut indices = Vec::new();
        let mut score = 0u32;
        for atom in &self.atoms {
            score += u32::from(atom.indices(text, &mut self.matcher, &mut indices)?);
        }
        indices.sort_unstable();
        indices.dedup();
        Some(Match { score, indices })
    }

    /// Scores `haystack` without computing highlight indices.
    pub(crate) fn score_only(&mut self, haystack: &str) -> Option<u32> {
        if self.empty {
            return Some(0);
        }
        let text = utf32(self.ascii.as_deref(), &mut self.buf, haystack)?;
        let mut score = 0u32;
        for atom in &self.atoms {
            score += u32::from(atom.score(text, &mut self.matcher)?);
        }
        Some(score)
    }
}

fn utf32<'a>(
    ascii: Option<&[Vec<u8>]>,
    buf: &'a mut Vec<char>,
    haystack: &'a str,
) -> Option<Utf32Str<'a>> {
    if haystack.is_ascii() {
        if ascii.is_some_and(|terms| {
            !terms
                .iter()
                .all(|term| ascii_subsequence(term, haystack.as_bytes()))
        }) {
            return None;
        }
        Some(Utf32Str::Ascii(haystack.as_bytes()))
    } else {
        buf.clear();
        buf.extend(haystack.chars());
        Some(Utf32Str::Unicode(buf.as_slice()))
    }
}

#[cfg(test)]
pub(crate) mod test_counters {
    use std::cell::Cell;

    thread_local! {
        static INDICES_COMPUTED: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn count_indices_computed() {
        INDICES_COMPUTED.with(|count| count.set(count.get() + 1));
    }

    pub(crate) fn indices_computed() -> usize {
        INDICES_COMPUTED.with(Cell::get)
    }

    pub(crate) fn reset_indices_computed() {
        INDICES_COMPUTED.with(|count| count.set(0));
    }
}

#[cfg(test)]
impl Fuzzy {
    fn unfiltered(query: &str) -> Self {
        Self {
            ascii: None,
            ..Self::new(query, false)
        }
    }
}

fn ascii_subsequence(term: &[u8], haystack: &[u8]) -> bool {
    let mut at = 0;
    for &byte in haystack {
        if at == term.len() {
            break;
        }
        if byte.to_ascii_lowercase() == term[at] {
            at += 1;
        }
    }
    at == term.len()
}

fn plain(query: &str) -> Vec<Atom> {
    query
        .split_whitespace()
        .map(|term| {
            Atom::new(
                term,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
                false,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
