//! Log line shapes: URLs, file paths and GUIDs, dispatched from log.rs.

/// Returns the byte length and role of a shape at `at`, or None.
pub(super) fn shape(bytes: &[u8], at: usize) -> Option<(usize, &'static str)> {
    let first = *bytes.get(at)?;
    if at > 0 && is_word(bytes[at - 1]) {
        return None;
    }
    match first {
        b'0'..=b'9' => date_or_time(bytes, at).or_else(|| guid_at(bytes, at)),
        b'\\' | b'/' => path(bytes, at).map(|len| (len, "link_uri")),
        b'{' => guid_at(bytes, at),
        b if b.is_ascii_alphabetic() => url(bytes, at)
            .map(|len| (len, "link_uri"))
            .or_else(|| path(bytes, at).map(|len| (len, "link_uri")))
            .or_else(|| guid_at(bytes, at)),
        _ => None,
    }
}

/// Returns a date or time shape at `at`, or None.
fn date_or_time(bytes: &[u8], at: usize) -> Option<(usize, &'static str)> {
    date(bytes, at)
        .or_else(|| time(bytes, at))
        .map(|len| (len, "attribute"))
}

/// Returns a GUID shape at `at`, or None.
fn guid_at(bytes: &[u8], at: usize) -> Option<(usize, &'static str)> {
    guid(bytes, at).map(|len| (len, "constructor"))
}

/// Returns the byte length of a URL at `at`, or None.
fn url(bytes: &[u8], at: usize) -> Option<usize> {
    if !matches!(bytes[at] | 0x20, b'h' | b'f') {
        return None;
    }
    let schemes = [
        b"http://".as_slice(),
        b"https://".as_slice(),
        b"ftp://".as_slice(),
        b"file://".as_slice(),
    ];
    let scheme = schemes
        .into_iter()
        .find(|scheme| starts_with_ignore_case(bytes, at, scheme))?;
    let mut end = at + scheme.len();
    while bytes.get(end).is_some_and(|&b| !is_url_end(b)) {
        end += 1;
    }
    while end > at + scheme.len() && matches!(bytes[end - 1], b'.' | b',' | b';' | b':') {
        end -= 1;
    }
    Some(end - at)
}

/// Returns the byte length of a date at `at`, or None.
fn date(bytes: &[u8], at: usize) -> Option<usize> {
    let end = year_first(bytes, at).or_else(|| day_first(bytes, at))?;
    if bytes.get(end) == Some(&b'T') {
        let time = time(bytes, end + 1)?;
        return Some(end + 1 + time - at);
    }
    boundary(bytes, end).then_some(end - at)
}

/// Returns the end of a `YYYY-MM-DD`, `YYYY/MM/DD` or `YYYY.MM.DD` date.
fn year_first(bytes: &[u8], at: usize) -> Option<usize> {
    if !four_digits(bytes, at) {
        return None;
    }
    let sep = *bytes.get(at + 4)?;
    if !matches!(sep, b'-' | b'/' | b'.') {
        return None;
    }
    let month = two_digits(bytes, at + 5)?;
    let day = two_digits(bytes, at + 8)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || bytes.get(at + 7) != Some(&sep) {
        return None;
    }
    Some(at + 10)
}

/// Returns the end of a `DD.MM.YYYY` or `DD/MM/YYYY` date at `at`.
fn day_first(bytes: &[u8], at: usize) -> Option<usize> {
    let day = two_digits(bytes, at)?;
    let sep = *bytes.get(at + 2)?;
    if !matches!(sep, b'.' | b'/') {
        return None;
    }
    let month = two_digits(bytes, at + 3)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || bytes.get(at + 5) != Some(&sep) {
        return None;
    }
    if !four_digits(bytes, at + 6) {
        return None;
    }
    Some(at + 10)
}

/// Returns the byte length of a time at `at`, or None.
fn time(bytes: &[u8], at: usize) -> Option<usize> {
    let (hour, after) = hour(bytes, at)?;
    if hour > 23 || bytes.get(after) != Some(&b':') {
        return None;
    }
    let minute = two_digits(bytes, after + 1)?;
    if minute > 59 {
        return None;
    }
    let mut end = after + 3;
    if bytes.get(end) == Some(&b':') {
        let second = two_digits(bytes, end + 1)?;
        if second > 59 {
            return None;
        }
        end = fraction(bytes, end + 3);
    }
    if bytes.get(end) == Some(&b'Z') {
        end += 1;
    }
    if matches!(bytes.get(end), Some(b'+' | b'-')) {
        end = zone(bytes, end + 1)?;
    }
    boundary(bytes, end).then_some(end - at)
}

/// Returns the hour value and the offset after it: one or two digits.
fn hour(bytes: &[u8], at: usize) -> Option<(u32, usize)> {
    let first = *bytes.get(at)?;
    if !first.is_ascii_digit() {
        return None;
    }
    let value = u32::from(first - b'0');
    match bytes.get(at + 1) {
        Some(&second) if second.is_ascii_digit() => {
            Some((value * 10 + u32::from(second - b'0'), at + 2))
        }
        _ => Some((value, at + 1)),
    }
}

/// Returns the end past a `[.,]` fraction with 1–9 digits, or `at`.
fn fraction(bytes: &[u8], at: usize) -> usize {
    if !matches!(bytes.get(at), Some(b'.' | b',')) {
        return at;
    }
    let mut digits = 0;
    while digits < 9 && bytes.get(at + 1 + digits).is_some_and(u8::is_ascii_digit) {
        digits += 1;
    }
    if digits == 0 { at } else { at + 1 + digits }
}

/// Returns the end past a `HH:MM` or `HHMM` zone after its sign at `at`.
fn zone(bytes: &[u8], at: usize) -> Option<usize> {
    let hour = two_digits(bytes, at)?;
    let mut end = at + 2;
    if bytes.get(end) == Some(&b':') {
        end += 1;
    }
    let minute = two_digits(bytes, end)?;
    (hour <= 23 && minute <= 59).then_some(end + 2)
}

/// Returns the byte length of a drive, UNC or POSIX path at `at`, or None.
fn path(bytes: &[u8], at: usize) -> Option<usize> {
    let drive = bytes.get(at).is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(at + 1) == Some(&b':')
        && matches!(bytes.get(at + 2), Some(b'\\' | b'/'));
    let unc = bytes.get(at) == Some(&b'\\')
        && bytes.get(at + 1) == Some(&b'\\')
        && bytes.get(at + 2).is_some_and(|&b| is_word(b));
    let posix = bytes.get(at) == Some(&b'/') && bytes.get(at + 1).is_some_and(|&b| is_word(b));
    if !(drive || unc || posix) {
        return None;
    }
    let mut end = at;
    while bytes.get(end).is_some_and(|&b| !is_path_end(b)) {
        end += 1;
    }
    while end > at + 1 && matches!(bytes[end - 1], b'.' | b',' | b':' | b';' | b')') {
        end -= 1;
    }
    if posix && !bytes[at + 1..end].contains(&b'/') {
        return None;
    }
    Some(end - at)
}

/// Returns the byte length of a bare or braced GUID at `at`, or None.
fn guid(bytes: &[u8], at: usize) -> Option<usize> {
    let braced = bytes.get(at) == Some(&b'{');
    let mut cursor = at + usize::from(braced);
    for (index, len) in [8, 4, 4, 4, 12].into_iter().enumerate() {
        if index > 0 {
            if bytes.get(cursor) != Some(&b'-') {
                return None;
            }
            cursor += 1;
        }
        let group = bytes.get(cursor..cursor + len)?;
        if !group.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        cursor += len;
    }
    if braced {
        if bytes.get(cursor) != Some(&b'}') {
            return None;
        }
        cursor += 1;
    }
    if bytes.get(cursor).is_some_and(|&b| is_word(b)) {
        return None;
    }
    Some(cursor - at)
}

/// Returns true when `bytes[at..]` starts with `prefix`, ignoring ASCII case.
fn starts_with_ignore_case(bytes: &[u8], at: usize, prefix: &[u8]) -> bool {
    bytes
        .get(at..at + prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Returns true for a byte that ends a URL run.
fn is_url_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b'<' | b'>' | b')' | b']' | b'}')
}

/// Returns true for a byte that ends a path run.
fn is_path_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b'<' | b'>' | b'|' | b'*' | b'?')
}

/// Returns the value of two ASCII digits at `at`, or None.
fn two_digits(bytes: &[u8], at: usize) -> Option<u32> {
    let pair = bytes.get(at..at + 2)?;
    if !pair.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(u32::from(pair[0] - b'0') * 10 + u32::from(pair[1] - b'0'))
}

/// Returns true when four ASCII digits start at `at`.
fn four_digits(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(at..at + 4)
        .is_some_and(|run| run.iter().all(u8::is_ascii_digit))
}

/// Returns true when a token may end at `at`: the chunk end or a non-word byte.
fn boundary(bytes: &[u8], at: usize) -> bool {
    bytes.get(at).is_none_or(|&b| !is_word(b))
}

/// Returns true for the word bytes of the Log lexer: ASCII alnum and `_`.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
