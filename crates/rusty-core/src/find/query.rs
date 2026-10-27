use std::ops::Range;

use super::filters::{
    AttributeFilter, ModifiedFilter, SizeFilter, parse_attrs, parse_modified, parse_size,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Ext,
    Size,
    Modified,
    Attrs,
    Within,
    Archives,
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenSpan {
    pub kind: TokenKind,
    pub span: Range<usize>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindQuery {
    pub text: String,
    pub case: bool,
    pub regex: bool,
    pub ext: Vec<String>,
    pub size: Option<SizeFilter>,
    pub modified: Option<ModifiedFilter>,
    pub attrs: Vec<AttributeFilter>,
    pub within: Option<String>,
    pub archives: bool,
    pub tokens: Vec<TokenSpan>,
}

impl FindQuery {
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let mut query = Self::default();
        for (span, closed) in split_tokens(input) {
            let source = &input[span.clone()];
            let word = source.replace('"', "");
            let kind = token_kind(source);
            let message = if !closed {
                Some("Unclosed quote".to_owned())
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

    fn apply_token(&mut self, kind: TokenKind, word: &str) -> Result<(), String> {
        if kind == TokenKind::Text {
            self.push_text(word);
            return Ok(());
        }
        let value = word.split_once(':').map_or(word, |(_, value)| value);
        match kind {
            TokenKind::Ext => self.ext.extend(parse_extensions(value)?),
            TokenKind::Size => {
                self.size = Some(parse_size(value).ok_or("size: needs a number, like <50MB")?);
            }
            TokenKind::Modified => {
                self.modified =
                    Some(parse_modified(value).ok_or("mod: use <7d, >2026-10-01 or today")?);
            }
            TokenKind::Attrs => self.attrs.extend(
                parse_attrs(value)
                    .ok_or("attr: use hidden, readonly, system or archive; prefix - to exclude")?,
            ),
            TokenKind::Within => {
                if value.is_empty() {
                    return Err("in: needs a folder".to_owned());
                }
                self.within = Some(value.to_owned());
            }
            TokenKind::Archives => {
                self.archives = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err("arc: use on or off".to_owned()),
                };
            }
            TokenKind::Regex => self.push_regex(value)?,
            TokenKind::Text => {}
        }
        Ok(())
    }

    fn push_regex(&mut self, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("re: needs a pattern".to_owned());
        }
        regex::Regex::new(value).map_err(|error| format!("re: {error}"))?;
        self.push_text(value);
        self.regex = true;
        Ok(())
    }

    fn push_text(&mut self, word: &str) {
        if word.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(word);
    }
}

fn token_kind(word: &str) -> TokenKind {
    if word == "-hidden" {
        return TokenKind::Attrs;
    }
    let Some((key, _)) = word.split_once(':') else {
        return TokenKind::Text;
    };
    match key.to_ascii_lowercase().as_str() {
        "ext" => TokenKind::Ext,
        "size" => TokenKind::Size,
        "mod" => TokenKind::Modified,
        "attr" => TokenKind::Attrs,
        "in" => TokenKind::Within,
        "arc" => TokenKind::Archives,
        "re" => TokenKind::Regex,
        _ => TokenKind::Text,
    }
}

fn parse_extensions(value: &str) -> Result<Vec<String>, String> {
    value
        .split(',')
        .map(|ext| {
            let ext = ext.trim_start_matches('.');
            if ext.is_empty() || ext.contains(['/', '\\']) {
                return Err("ext: needs extensions, like c,h,map".to_owned());
            }
            Ok(ext.to_lowercase())
        })
        .collect()
}

fn split_tokens(input: &str) -> Vec<(Range<usize>, bool)> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() && !quoted {
            if let Some(start) = start.take() {
                tokens.push((start..index, true));
            }
            continue;
        }
        start.get_or_insert(index);
        if ch == '"' {
            quoted = !quoted;
        }
    }
    if let Some(start) = start {
        tokens.push((start..input.len(), !quoted));
    }
    tokens
}
