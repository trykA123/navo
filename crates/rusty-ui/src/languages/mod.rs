mod ambiguous;
mod captures;
mod grammars;
mod names;
mod sniff;

pub use grammars::register_languages;

#[must_use]
pub fn detect(name: &str, head: &[u8]) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if let Some(language) = names::special(&lower) {
        return language;
    }
    let ext = lower
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or("", |(_, ext)| ext);
    if let Some(resolve) = ambiguous::resolver(ext) {
        let text = sniff::head_text(head);
        return resolve(&text)
            .or_else(|| sniff::content(&text))
            .unwrap_or(names::TEXT);
    }
    names::by_extension(ext)
        .or_else(|| sniff::content(&sniff::head_text(head)))
        .unwrap_or(names::TEXT)
}

#[cfg(test)]
mod tests;
