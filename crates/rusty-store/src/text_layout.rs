pub(crate) const BOM: char = '\u{feff}';

pub(crate) fn keep_layout<E>(
    text: &str,
    edit: impl FnOnce(&str) -> Result<String, E>,
) -> Result<String, E> {
    let body = text.strip_prefix(BOM);
    let is_crlf = text.contains("\r\n");
    let plain = body.unwrap_or(text).replace("\r\n", "\n");
    let edited = edit(&plain)?;
    let edited = if is_crlf {
        edited.replace('\n', "\r\n")
    } else {
        edited
    };
    Ok(match body {
        Some(_) => format!("{BOM}{edited}"),
        None => edited,
    })
}
