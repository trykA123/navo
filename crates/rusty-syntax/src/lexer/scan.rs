pub(super) fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn word_len(text: &str) -> usize {
    text.find(|ch: char| !is_word(ch)).unwrap_or(text.len())
}

pub(super) fn starts_with(text: &str, prefix: &str, case_ignored: bool) -> bool {
    match text.as_bytes().get(..prefix.len()) {
        Some(head) if case_ignored => head.eq_ignore_ascii_case(prefix.as_bytes()),
        Some(head) => head == prefix.as_bytes(),
        None => false,
    }
}

pub(super) fn after_word(text: &str, len: usize, word: &str) -> bool {
    !word.chars().next_back().is_some_and(is_word)
        || !text[len..].chars().next().is_some_and(is_word)
}

fn digits(bytes: &[u8], from: usize, accept: fn(u8) -> bool) -> usize {
    bytes[from..].iter().take_while(|b| accept(**b)).count()
}

pub(super) fn number_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let radix: Option<fn(u8) -> bool> = match bytes.get(..2) {
        Some(b"0x" | b"0X") => Some(|b| b.is_ascii_hexdigit()),
        Some(b"0b" | b"0B") => Some(|b| matches!(b, b'0' | b'1')),
        _ => None,
    };
    let mut end = match radix {
        Some(accept) => match digits(bytes, 2, accept) {
            0 => return None,
            count => 2 + count,
        },
        None => decimal_len(bytes),
    };
    end += digits(bytes, end, |b| {
        matches!(b, b'u' | b'U' | b'l' | b'L' | b'f' | b'F')
    });
    if text[end..].chars().next().is_some_and(is_word) {
        return None;
    }
    Some(end)
}

fn decimal_len(bytes: &[u8]) -> usize {
    let mut end = digits(bytes, 0, |b| b.is_ascii_digit());
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1 + digits(bytes, end + 1, |b| b.is_ascii_digit());
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(bytes, end + 1 + sign, |b| b.is_ascii_digit());
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    end
}
