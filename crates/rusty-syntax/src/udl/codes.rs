use crate::{Delimiter, KeywordGroup, Language, RulesError, TokenKind};

const GROUP_KINDS: [TokenKind; 8] = [
    TokenKind::Keyword,
    TokenKind::Type,
    TokenKind::Function,
    TokenKind::Constant,
    TokenKind::Attribute,
    TokenKind::Tag,
    TokenKind::Property,
    TokenKind::Variable,
];
const EOL: &str = "((EOL))";

#[derive(Default)]
pub(super) struct Draft {
    name: Option<String>,
    extensions: Vec<String>,
    pub(super) case_ignored: bool,
    pub(super) prefix: [bool; 8],
    groups: [Vec<String>; 8],
    comments: Vec<Vec<String>>,
    delimiters: Vec<Vec<String>>,
    operators: Vec<String>,
}

impl Draft {
    pub(super) fn new(name: Option<String>, extensions: &str) -> Self {
        Self {
            name,
            extensions: extensions
                .split_whitespace()
                .map(|ext| ext.trim_start_matches('.').to_lowercase())
                .collect(),
            ..Self::default()
        }
    }

    pub(super) fn keywords(&mut self, name: &str, text: &str) {
        let group = name
            .strip_prefix("Keywords")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| self.groups.get_mut(n));
        if let Some(group) = group {
            *group = words(text);
            return;
        }
        match name {
            "Comments" => self.comments = coded(text, 5),
            "Delimiters" => self.delimiters = coded(text, 24),
            "Operators1" | "Operators2" => self.operators.extend(words(text)),
            _ => {}
        }
    }

    pub(super) fn finish(self) -> Result<Language, RulesError> {
        let name = self
            .name
            .filter(|name| !name.trim().is_empty())
            .ok_or(RulesError::MissingName)?;
        let keywords = self
            .groups
            .into_iter()
            .zip(GROUP_KINDS)
            .zip(self.prefix)
            .filter(|((words, _), _)| !words.is_empty())
            .map(|((words, kind), prefix)| KeywordGroup {
                kind,
                words,
                prefix,
            })
            .collect();
        let block_comments = code(&self.comments, 3)
            .into_iter()
            .zip(code(&self.comments, 4))
            .collect();
        let delimiters = (0..8)
            .filter_map(|group| delimiter(&self.delimiters, group))
            .collect();
        Ok(Language {
            name,
            extensions: self.extensions,
            case_ignored: self.case_ignored,
            keywords,
            line_comments: code(&self.comments, 0),
            block_comments,
            delimiters,
            operators: self.operators,
            numbers: true,
        })
    }
}

fn code(codes: &[Vec<String>], index: usize) -> Vec<String> {
    codes.get(index).cloned().unwrap_or_default()
}

fn delimiter(codes: &[Vec<String>], group: usize) -> Option<Delimiter> {
    let open = code(codes, group * 3).into_iter().next()?;
    let escape = code(codes, group * 3 + 1)
        .first()
        .and_then(|value| value.chars().next());
    let close = code(codes, group * 3 + 2)
        .into_iter()
        .next()
        .unwrap_or_default();
    let to_eol = close.is_empty() || close == EOL;
    Some(Delimiter {
        open,
        close: if to_eol { String::new() } else { close },
        escape,
        multiline: !to_eol,
        kind: TokenKind::String,
    })
}

fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(end) = quoted.find('"')
        {
            words.push(quoted[..end].to_owned());
            rest = quoted[end + 1..].trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        words.push(rest[..end].to_owned());
        rest = rest[end..].trim_start();
    }
    words
}

fn coded(text: &str, codes: usize) -> Vec<Vec<String>> {
    let mut values = vec![Vec::new(); codes];
    let mut current = None;
    for word in text.split_whitespace() {
        let code = word
            .get(..2)
            .filter(|digits| digits.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|digits| digits.parse::<usize>().ok())
            .filter(|code| *code < codes);
        let value = match code {
            Some(code) => {
                current = Some(code);
                &word[2..]
            }
            None => word,
        };
        if let Some(slot) = current.and_then(|code| values.get_mut(code))
            && !value.is_empty()
        {
            slot.push(value.to_owned());
        }
    }
    values
}
