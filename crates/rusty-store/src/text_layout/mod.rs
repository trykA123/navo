mod endings;

pub(crate) const BOM: char = '\u{feff}';

pub(crate) fn plain_text(text: &str) -> String {
    text.strip_prefix(BOM).unwrap_or(text).replace("\r\n", "\n")
}

pub(crate) fn keep_layout<E>(
    text: &str,
    edit: impl FnOnce(&str) -> Result<String, E>,
) -> Result<String, E> {
    let edited = edit(&plain_text(text))?;
    Ok(restore_layout(text, &edited))
}

pub(crate) fn restore_layout(original: &str, edited: &str) -> String {
    let body = original.strip_prefix(BOM);
    let lines = endings::restore(body.unwrap_or(original), edited);
    match body {
        Some(_) => format!("{BOM}{lines}"),
        None => lines,
    }
}

#[cfg(test)]
mod tests;
