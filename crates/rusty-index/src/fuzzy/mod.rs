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
        let text = if haystack.is_ascii() {
            Utf32Str::Ascii(haystack.as_bytes())
        } else {
            self.buf.clear();
            self.buf.extend(haystack.chars());
            Utf32Str::Unicode(&self.buf)
        };
        let mut indices = Vec::new();
        let mut score = 0u32;
        for atom in &self.atoms {
            score += u32::from(atom.indices(text, &mut self.matcher, &mut indices)?);
        }
        indices.sort_unstable();
        indices.dedup();
        Some(Match { score, indices })
    }
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
