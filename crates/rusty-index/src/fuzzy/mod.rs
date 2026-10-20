use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: u32,
    pub indices: Vec<u32>,
}

pub struct Fuzzy {
    matcher: Matcher,
    pattern: Pattern,
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
            pattern: Pattern::parse(query, CaseMatching::Smart, Normalization::Smart),
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
        let text = Utf32Str::new(haystack, &mut self.buf);
        let mut indices = Vec::new();
        let score = self
            .pattern
            .indices(text, &mut self.matcher, &mut indices)?;
        indices.sort_unstable();
        indices.dedup();
        Some(Match { score, indices })
    }
}

#[cfg(test)]
mod tests;
