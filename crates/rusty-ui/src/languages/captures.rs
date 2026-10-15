const EXACT: [(&str, &str); 5] = [
    ("markup.heading", "title"),
    ("markup.link", "link_uri"),
    ("markup.raw", "text.literal"),
    ("text.uri", "link_uri"),
    ("delimiter", "punctuation.delimiter"),
];

const FAMILIES: [(&str, &str); 12] = [
    ("conditional", "keyword"),
    ("repeat", "keyword"),
    ("include", "keyword"),
    ("exception", "keyword"),
    ("storageclass", "keyword"),
    ("define", "keyword"),
    ("method", "function"),
    ("parameter", "variable"),
    ("field", "property"),
    ("float", "number"),
    ("character", "string"),
    ("namespace", "type"),
];

pub(super) fn remap(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut rest = query;
    while let Some(at) = next_capture(rest) {
        out.push_str(&rest[..=at]);
        let tail = &rest[at + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
            .unwrap_or(tail.len());
        out.push_str(&rename(&tail[..end]));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn next_capture(text: &str) -> Option<usize> {
    let mut in_string = false;
    let mut in_comment = false;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\n' => in_comment = false,
            _ if in_comment => {}
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            ';' if !in_string => in_comment = true,
            '@' if !in_string => return Some(at),
            _ => {}
        }
    }
    None
}

fn rename(name: &str) -> String {
    if let Some((_, to)) = EXACT.iter().find(|(from, _)| *from == name) {
        return (*to).to_owned();
    }
    let (family, suffix) = name.split_once('.').map_or((name, ""), |(f, s)| (f, s));
    match FAMILIES.iter().find(|(from, _)| *from == family) {
        Some((_, to)) if suffix.is_empty() => (*to).to_owned(),
        Some((_, to)) => format!("{to}.{suffix}"),
        None => name.to_owned(),
    }
}
