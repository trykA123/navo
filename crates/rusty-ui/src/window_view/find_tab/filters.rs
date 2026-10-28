use rusty_core::find::{Attribute, FindQuery, ModifiedFilter, SizeFilter, TokenKind};

pub(super) fn text(query: &FindQuery) -> String {
    let mut filters = Vec::new();
    for token in &query.tokens {
        if token.message.is_some() {
            continue;
        }
        let value = match token.kind {
            TokenKind::Ext if !query.ext.is_empty() => Some(format!("ext:{}", query.ext.join(","))),
            TokenKind::Size => query.size.map(size),
            TokenKind::Modified => query.modified.map(modified),
            TokenKind::Attrs => Some(attributes(query)),
            TokenKind::Within => query.within.as_ref().map(|path| format!("in:{path}")),
            TokenKind::Archives => {
                Some(format!("arc:{}", if query.archives { "on" } else { "off" }))
            }
            _ => None,
        };
        if let Some(value) = value.filter(|value| !value.is_empty() && !filters.contains(value)) {
            filters.push(value);
        }
    }
    if query.case {
        filters.push(rusty_l10n::t!("command-center-content-case"));
    }
    if query.regex {
        filters.push(rusty_l10n::t!("command-center-content-regex"));
    }
    filters.join(" ")
}

fn size(filter: SizeFilter) -> String {
    match filter {
        SizeFilter::LessThan(bytes) => format!("size:<{bytes}"),
        SizeFilter::GreaterThan(bytes) => format!("size:>{bytes}"),
        SizeFilter::Range(start, end) => format!("size:{start}..{end}"),
    }
}

fn modified(filter: ModifiedFilter) -> String {
    match filter {
        ModifiedFilter::YoungerThanDays(days) => format!("mod:<{days}d"),
        ModifiedFilter::OlderThanDays(days) => format!("mod:>{days}d"),
        ModifiedFilter::Today => "mod:today".to_owned(),
        ModifiedFilter::Before(date) | ModifiedFilter::After(date) => {
            let sign = if matches!(filter, ModifiedFilter::Before(_)) {
                '<'
            } else {
                '>'
            };
            format!(
                "mod:{sign}{:04}-{:02}-{:02}",
                date.year, date.month, date.day
            )
        }
    }
}

fn attributes(query: &FindQuery) -> String {
    let names = query
        .attrs
        .iter()
        .map(|filter| {
            let name = match filter.attribute {
                Attribute::Hidden => "hidden",
                Attribute::Readonly => "readonly",
                Attribute::System => "system",
                Attribute::Archive => "archive",
            };
            format!("{}{name}", if filter.excluded { "-" } else { "" })
        })
        .collect::<Vec<_>>();
    format!("attr:{}", names.join(","))
}
