//! Name filter for `name:` masks (FR-D10): `;`-separated wildcard masks with `!` excludes, or one
//! case-insensitive `/regex/` on the file name.

use regex::{Regex, RegexBuilder};

use crate::selection::Pattern;

/// File-name filter compiled from a `name:` value.
pub struct NameFilter {
    rule: Rule,
}

enum Rule {
    Regex(Regex),
    Masks {
        includes: Vec<Pattern>,
        excludes: Vec<Pattern>,
    },
}

impl NameFilter {
    /// Compiles a `name:` value; a bad regex or an empty `!` part is an error.
    pub fn new(mask: &str) -> Result<Self, String> {
        let mask = mask.trim();
        let rule = match regex_body(mask) {
            Some(body) => Rule::Regex(
                RegexBuilder::new(body)
                    .case_insensitive(true)
                    .build()
                    .map_err(|error| error.to_string())?,
            ),
            None => masks(mask)?,
        };
        Ok(Self { rule })
    }

    /// True when `name` matches one include (or there are none) and no exclude.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        match &self.rule {
            Rule::Regex(regex) => regex.is_match(name),
            Rule::Masks { includes, excludes } => {
                (includes.is_empty() || includes.iter().any(|mask| mask.matches(name)))
                    && !excludes.iter().any(|mask| mask.matches(name))
            }
        }
    }
}

fn regex_body(mask: &str) -> Option<&str> {
    (mask.len() > 2)
        .then(|| mask.strip_prefix('/')?.strip_suffix('/'))
        .flatten()
}

fn masks(mask: &str) -> Result<Rule, String> {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    for part in mask
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.strip_prefix('!') {
            Some(rest) => excludes.push(pattern(rest.trim())?),
            None => includes.push(pattern(part)?),
        }
    }
    Ok(Rule::Masks { includes, excludes })
}

/// Compiles one mask part; a bare word matches inside names (`*word*`) and an empty part (a bare
/// `!`) is an error.
fn pattern(part: &str) -> Result<Pattern, String> {
    if part.is_empty() {
        return Err("name: a `!` needs a mask".to_owned());
    }
    let result = if part.contains(['*', '?']) {
        Pattern::parse(part)
    } else {
        Pattern::parse(&format!("*{part}*"))
    };
    result.map_err(|error| error.to_string())
}
