use crate::{RulesError, TokenKind};

/// Longest comment opener, closer or string delimiter a language may have, in bytes. A chunk end
/// can cut only so many bytes of a pattern, which keeps the carried state small.
pub const MAX_PATTERN_BYTES: usize = 64;

/// Most openers (line comments, block comments and delimiters together) a language may have;
/// the chunk state numbers them with 16 bits.
pub const MAX_OPENERS: usize = 65_535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub name: String,
    pub extensions: Vec<String>,
    pub case_ignored: bool,
    pub keywords: Vec<KeywordGroup>,
    pub line_comments: Vec<String>,
    pub block_comments: Vec<(String, String)>,
    pub delimiters: Vec<Delimiter>,
    pub operators: Vec<String>,
    pub numbers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordGroup {
    pub kind: TokenKind,
    pub words: Vec<String>,
    pub prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delimiter {
    pub open: String,
    pub close: String,
    pub escape: Option<char>,
    pub multiline: bool,
    pub kind: TokenKind,
}

impl Language {
    /// Checks the limits the chunked lexer relies on: every opener, closer and line comment is at
    /// most [`MAX_PATTERN_BYTES`] long, and there are at most [`MAX_OPENERS`] openers.
    ///
    /// # Errors
    /// Returns [`RulesError::Limit`] naming the first limit the language breaks.
    pub fn check_limits(&self) -> Result<(), RulesError> {
        let blocks = self
            .block_comments
            .iter()
            .flat_map(|(open, close)| [open, close]);
        let strings = self.delimiters.iter().flat_map(|d| [&d.open, &d.close]);
        let patterns = self.line_comments.iter().chain(blocks).chain(strings);
        let limit = |what: String| RulesError::Limit {
            name: self.name.clone(),
            what,
        };
        if let Some(long) = patterns.into_iter().find(|p| p.len() > MAX_PATTERN_BYTES) {
            let head: String = long.chars().take(16).collect();
            return Err(limit(format!(
                "`{head}...` is over {MAX_PATTERN_BYTES} bytes long"
            )));
        }
        let openers = self.line_comments.len() + self.block_comments.len() + self.delimiters.len();
        if openers > MAX_OPENERS {
            return Err(limit(format!(
                "{openers} comment and string openers, over {MAX_OPENERS}"
            )));
        }
        Ok(())
    }
}
