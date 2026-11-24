//! Canonical text for a find query.

use super::{Attribute, AttributeFilter, FindQuery, GitRules, ModifiedFilter, SizeFilter};

impl FindQuery {
    /// Prints the query as one canonical line that [`FindQuery::parse`] reads back.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        if !self.text.is_empty() {
            if self.regex {
                parts.push(format!("re:{}", quoted(&self.text)));
            } else {
                parts.push(self.text.clone());
            }
        }
        if !self.ext.is_empty() {
            parts.push(format!("ext:{}", self.ext.join(",")));
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
        if let Some(within) = &self.within {
            parts.push(format!("in:{}", quoted(within)));
        }
        if self.archives {
            parts.push("arc:on".to_owned());
        }
        if self.git_rules == GitRules::Off {
            parts.push("git:off".to_owned());
        }
        parts.join(" ")
    }
}

/// Wraps a value that would split into two tokens or carry a stray colon.
fn quoted(value: &str) -> String {
    if value.contains(char::is_whitespace) || value.contains(':') {
        format!("\"{value}\"")
    } else {
        value.to_owned()
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
