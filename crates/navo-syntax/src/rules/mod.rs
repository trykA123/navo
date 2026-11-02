use serde::Deserialize;

use crate::{Delimiter, KeywordGroup, Language, RulesError, TokenKind};

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    language: Vec<RawLanguage>,
}

#[derive(Deserialize)]
struct RawLanguage {
    name: Option<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    case_ignored: bool,
    #[serde(default)]
    keywords: Vec<RawGroup>,
    #[serde(default)]
    line_comments: Vec<String>,
    #[serde(default)]
    block_comments: Vec<(String, String)>,
    #[serde(default)]
    delimiters: Vec<RawDelimiter>,
    #[serde(default)]
    operators: Vec<String>,
    #[serde(default = "yes")]
    numbers: bool,
}

#[derive(Deserialize)]
struct RawGroup {
    kind: TokenKind,
    words: Vec<String>,
    #[serde(default)]
    prefix: bool,
}

#[derive(Deserialize)]
struct RawDelimiter {
    open: String,
    #[serde(default)]
    close: String,
    escape: Option<char>,
    #[serde(default)]
    multiline: bool,
    #[serde(default = "string_kind")]
    kind: TokenKind,
}

fn yes() -> bool {
    true
}

fn string_kind() -> TokenKind {
    TokenKind::String
}

pub fn from_toml(src: &str) -> Result<Vec<Language>, RulesError> {
    let deserializer = toml::Deserializer::parse(src).map_err(|error| RulesError::Toml {
        message: error.to_string(),
    })?;
    let mut unknown = None;
    let file: File = serde_ignored::deserialize(deserializer, |path| {
        unknown.get_or_insert_with(|| path.to_string());
    })
    .map_err(|error: toml::de::Error| RulesError::Toml {
        message: error.to_string(),
    })?;
    if let Some(key) = unknown {
        return Err(RulesError::UnknownKey { key });
    }
    file.language.into_iter().map(language).collect()
}

fn language(raw: RawLanguage) -> Result<Language, RulesError> {
    let name = raw
        .name
        .filter(|name| !name.trim().is_empty())
        .ok_or(RulesError::MissingName)?;
    Ok(Language {
        name,
        extensions: raw
            .extensions
            .iter()
            .map(|ext| ext.trim_start_matches('.').to_lowercase())
            .collect(),
        case_ignored: raw.case_ignored,
        keywords: raw
            .keywords
            .into_iter()
            .map(|group| KeywordGroup {
                kind: group.kind,
                words: group.words,
                prefix: group.prefix,
            })
            .collect(),
        line_comments: raw.line_comments,
        block_comments: raw.block_comments,
        delimiters: raw
            .delimiters
            .into_iter()
            .map(|delimiter| Delimiter {
                open: delimiter.open,
                close: delimiter.close,
                escape: delimiter.escape,
                multiline: delimiter.multiline,
                kind: delimiter.kind,
            })
            .collect(),
        operators: raw.operators,
        numbers: raw.numbers,
    })
}

#[cfg(test)]
mod tests;
