#[must_use]
pub fn is_redacted_path(text: &str) -> bool {
    let Some((has_root, rest)) = split_root(text) else {
        return false;
    };
    if rest.is_empty() {
        return has_root;
    }
    rest.split(['\\', '/']).all(is_token)
}

fn split_root(text: &str) -> Option<(bool, &str)> {
    if let Some(rest) = text.strip_prefix(r"\\") {
        return unc_rest(rest).map(|rest| (true, rest));
    }
    if let Some(rest) = text.strip_prefix('/') {
        return Some((true, rest));
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_uppercase() && &bytes[1..3] == b":\\" {
        return Some((true, &text[3..]));
    }
    Some((false, text))
}

fn unc_rest(text: &str) -> Option<&str> {
    let rest = numbered(text, "server")?;
    let rest = rest.strip_prefix('\\')?;
    let rest = numbered(rest, "share")?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix('\\').filter(|rest| !rest.is_empty())
}

fn numbered<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let rest = text
        .strip_prefix('<')?
        .strip_prefix(tag)?
        .strip_prefix(':')?;
    let (number, rest) = rest.split_once('>')?;
    let digits = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    digits.then_some(rest)
}

fn is_token(token: &str) -> bool {
    if token == "<user>" {
        return true;
    }
    if let Some(hex) = token.strip_prefix("<d:").and_then(|t| t.strip_suffix('>')) {
        return is_hex4(hex);
    }
    let Some(rest) = token.strip_prefix("<f:") else {
        return false;
    };
    match rest.split_once('>') {
        Some((hex, "")) => is_hex4(hex),
        Some((hex, ext)) => is_hex4(hex) && is_extension(ext),
        None => false,
    }
}

fn is_hex4(text: &str) -> bool {
    text.len() == 4 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_extension(text: &str) -> bool {
    text.strip_prefix('.').is_some_and(|ext| {
        ext.bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
            && crate::redact::is_allowed_extension(ext)
    })
}
