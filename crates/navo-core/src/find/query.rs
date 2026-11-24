use std::ops::Range;

use super::filters::{
    AttributeFilter, ModifiedFilter, SizeFilter, parse_attrs, parse_modified, parse_size,
};
use super::tokens::{self, TextEncoding, TokenError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Ext,
    Size,
    Modified,
    Attrs,
    Within,
    Archives,
    Git,
    Regex,
    Name,
    Depth,
    Word,
    Not,
    Case,
    Hex,
    Enc,
    Docs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenSpan {
    pub kind: TokenKind,
    pub span: Range<usize>,
    pub message: Option<TokenError>,
}

/// Whether find in files skips what `.gitignore` files list (`git:on`, the default) or not (`git:off`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GitRules {
    #[default]
    On,
    Off,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindQuery {
    pub text: String,
    pub case: bool,
    pub regex: bool,
    pub name: Option<String>,
    pub depth: Option<u32>,
    pub word: bool,
    pub invert: bool,
    pub hex: Option<Vec<u8>>,
    pub encodings: Vec<TextEncoding>,
    pub docs: bool,
    pub ext: Vec<String>,
    pub size: Option<SizeFilter>,
    pub modified: Option<ModifiedFilter>,
    pub attrs: Vec<AttributeFilter>,
    pub within: Vec<String>,
    pub archives: bool,
    pub git_rules: GitRules,
    pub tokens: Vec<TokenSpan>,
}

impl FindQuery {
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let mut query = Self::default();
        for (span, closed) in tokens::split_tokens(input) {
            let source = &input[span.clone()];
            let word = tokens::unquote(source);
            let kind = tokens::token_kind(source);
            let message = if !closed {
                Some(TokenError::UnclosedQuote)
            } else {
                query.apply_token(kind, &word).err()
            };
            query.tokens.push(TokenSpan {
                kind,
                span,
                message,
            });
        }
        query
    }

    fn apply_token(&mut self, kind: TokenKind, word: &str) -> Result<(), TokenError> {
        let value = word.split_once(':').map_or(word, |(_, value)| value);
        match kind {
            TokenKind::Name
            | TokenKind::Depth
            | TokenKind::Word
            | TokenKind::Not
            | TokenKind::Case
            | TokenKind::Hex
            | TokenKind::Enc
            | TokenKind::Docs => tokens::apply(self, kind, value)?,
            TokenKind::Text => self.push_text(word)?,
            TokenKind::Ext => self.ext.extend(parse_extensions(value)?),
            TokenKind::Size => {
                self.size = Some(parse_size(value).ok_or(TokenError::Size)?);
            }
            TokenKind::Modified => {
                self.modified = Some(parse_modified(value).ok_or(TokenError::Modified)?);
            }
            TokenKind::Attrs => self
                .attrs
                .extend(parse_attrs(value).ok_or(TokenError::Attribute)?),
            TokenKind::Within => {
                if value.is_empty() {
                    return Err(TokenError::Within);
                }
                self.within.push(value.to_owned());
            }
            TokenKind::Archives => {
                self.archives = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(TokenError::Archives),
                };
            }
            TokenKind::Git => {
                self.git_rules = match value {
                    "on" => GitRules::On,
                    "off" => GitRules::Off,
                    _ => return Err(TokenError::Git),
                };
            }
            TokenKind::Regex => self.push_regex(value)?,
        }
        Ok(())
    }

    fn push_regex(&mut self, value: &str) -> Result<(), TokenError> {
        if value.is_empty() {
            return Err(TokenError::Regex);
        }
        let syntax = |error: regex::Error| TokenError::RegexSyntax(error.to_string());
        regex::Regex::new(value).map_err(syntax)?;
        let text = self.composed(value);
        regex::Regex::new(&text).map_err(syntax)?;
        self.text = text;
        self.regex = true;
        Ok(())
    }

    fn push_text(&mut self, word: &str) -> Result<(), TokenError> {
        if word.is_empty() {
            return Ok(());
        }
        let text = self.composed(word);
        if self.regex {
            regex::Regex::new(&text).map_err(|error| TokenError::RegexSyntax(error.to_string()))?;
        }
        self.text = text;
        Ok(())
    }

    fn composed(&self, word: &str) -> String {
        let mut text = self.text.clone();
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
        text
    }
}

fn parse_extensions(value: &str) -> Result<Vec<String>, TokenError> {
    value
        .split(',')
        .map(|ext| {
            let ext = ext.trim_start_matches('.');
            if ext.is_empty() || ext.contains(['/', '\\']) {
                return Err(TokenError::Extensions);
            }
            Ok(ext.to_lowercase())
        })
        .collect()
}
