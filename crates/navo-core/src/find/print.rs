//! Canonical text for a find query.

use super::query::token_kind;
use super::tokens::TextEncoding;
use super::{
    Attribute, AttributeFilter, FindQuery, GitRules, ModifiedFilter, SizeFilter, TokenKind,
};

impl FindQuery {
    /// Prints the query as one canonical line that [`FindQuery::parse`] reads back.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        if !self.text.is_empty() {
            if self.regex {
                parts.push(format!("re:{}", quoted_value(&self.text)));
            } else {
                parts.push(plain_text(&self.text));
            }
        }
        if !self.ext.is_empty() {
            parts.push(format!("ext:{}", quoted_value(&self.ext.join(","))));
        }
        if let Some(size) = self.size {
            parts.push(size_text(size));
        }
        if let Some(modified) = self.modified {
            parts.push(modified_text(modified));
        }
        if !self.attrs.is_empty() {
            let names = self.attrs.iter().map(attribute_text).collect::<Vec<_>>();
            parts.push(format!("attr:{}", names.join(",")));
        }
        for within in &self.within {
            parts.push(format!("in:{}", quoted_value(within)));
        }
        if self.archives {
            parts.push("arc:on".to_owned());
        }
        if self.git_rules == GitRules::Off {
            parts.push("git:off".to_owned());
        }
        if let Some(name) = &self.name {
            parts.push(format!("name:{}", quoted_value(name)));
        }
        if let Some(depth) = self.depth {
            parts.push(format!("depth:{depth}"));
        }
        if self.word {
            parts.push("word:on".to_owned());
        }
        if self.invert {
            parts.push("not:on".to_owned());
        }
        if self.case {
            parts.push("case:on".to_owned());
        }
        if let Some(hex) = &self.hex {
            parts.push(format!("hex:{}", hex_text(hex)));
        }
        if !self.encodings.is_empty() {
            let names = self
                .encodings
                .iter()
                .map(|encoding| encoding_text(*encoding))
                .collect::<Vec<_>>();
            parts.push(format!("enc:{}", names.join(",")));
        }
        if self.docs {
            parts.push("docs:on".to_owned());
        }
        parts.join(" ")
    }
}

/// Wraps a token value that holds whitespace, a colon or a quote.
fn quoted_value(value: &str) -> String {
    if value.contains(char::is_whitespace) || value.contains(':') || value.contains('"') {
        format!("\"{}\"", escaped(value))
    } else {
        value.to_owned()
    }
}

/// Prints plain search text so that it reparses as the same text, not as filters.
fn plain_text(text: &str) -> String {
    if text_needs_quoting(text) {
        format!("\"{}\"", escaped(text))
    } else {
        text.to_owned()
    }
}

fn text_needs_quoting(text: &str) -> bool {
    text.starts_with(' ')
        || text.ends_with(' ')
        || text.contains("  ")
        || text.contains('"')
        || text.chars().any(|ch| ch.is_whitespace() && ch != ' ')
        || text
            .split(' ')
            .any(|word| !matches!(token_kind(word), TokenKind::Text))
}

/// Escapes a literal double quote for the tokenizer.
fn escaped(value: &str) -> String {
    value.replace('"', "\\\"")
}

fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn encoding_text(encoding: TextEncoding) -> &'static str {
    match encoding {
        TextEncoding::Auto => "auto",
        TextEncoding::Ansi => "ansi",
        TextEncoding::Dos => "dos",
        TextEncoding::Utf8 => "utf8",
        TextEncoding::Utf16 => "utf16",
    }
}

fn size_text(filter: SizeFilter) -> String {
    match filter {
        SizeFilter::LessThan(bytes) => format!("size:<{bytes}"),
        SizeFilter::GreaterThan(bytes) => format!("size:>{bytes}"),
        SizeFilter::Range(start, end) => format!("size:{start}..{end}"),
    }
}

fn modified_text(filter: ModifiedFilter) -> String {
    match filter {
        ModifiedFilter::YoungerThanDays(days) => format!("mod:<{days}d"),
        ModifiedFilter::OlderThanDays(days) => format!("mod:>{days}d"),
        ModifiedFilter::Today => "mod:today".to_owned(),
        ModifiedFilter::Before(date) => date_text('<', date),
        ModifiedFilter::After(date) => date_text('>', date),
    }
}

fn date_text(sign: char, date: super::Date) -> String {
    format!(
        "mod:{sign}{:04}-{:02}-{:02}",
        date.year, date.month, date.day
    )
}

fn attribute_text(filter: &AttributeFilter) -> String {
    let name = match filter.attribute {
        Attribute::Hidden => "hidden",
        Attribute::Readonly => "readonly",
        Attribute::System => "system",
        Attribute::Archive => "archive",
    };
    format!("{}{name}", if filter.excluded { "-" } else { "" })
}
