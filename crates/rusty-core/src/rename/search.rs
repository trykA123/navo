use regex::{NoExpand, Regex, RegexBuilder};

use super::rules::Find;

pub(super) struct Search {
    pairs: Vec<Pair>,
    first_only: bool,
}

struct Pair {
    regex: Regex,
    replace: String,
    literal: bool,
}

impl Search {
    pub(super) fn new(find: &Find) -> Result<Self, String> {
        let pairs = if find.regex {
            regex_pairs(find)?
        } else {
            plain_pairs(find)?
        };
        Ok(Self {
            pairs,
            first_only: find.first_only,
        })
    }

    pub(super) fn apply(&self, text: &str) -> String {
        let limit = usize::from(self.first_only);
        let mut current = text.to_owned();
        for pair in &self.pairs {
            current = if pair.literal {
                pair.regex
                    .replacen(&current, limit, NoExpand(&pair.replace))
                    .into_owned()
            } else {
                pair.regex
                    .replacen(&current, limit, pair.replace.as_str())
                    .into_owned()
            };
        }
        current
    }
}

fn build(pattern: &str, match_case: bool) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(!match_case)
        .build()
        .map_err(|e| e.to_string())
}

fn regex_pairs(find: &Find) -> Result<Vec<Pair>, String> {
    if find.text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Pair {
        regex: build(&find.text, find.match_case)?,
        replace: find.replace.clone(),
        literal: false,
    }])
}

fn plain_pairs(find: &Find) -> Result<Vec<Pair>, String> {
    let replacements: Vec<&str> = find.replace.split('|').collect();
    let mut pairs = Vec::new();
    for (index, needle) in find.text.split('|').enumerate() {
        if needle.is_empty() {
            continue;
        }
        let replace = match replacements.as_slice() {
            [only] => only,
            many => many.get(index).copied().unwrap_or(""),
        };
        pairs.push(Pair {
            regex: build(&regex::escape(needle), find.match_case)?,
            replace: replace.to_owned(),
            literal: true,
        });
    }
    Ok(pairs)
}
