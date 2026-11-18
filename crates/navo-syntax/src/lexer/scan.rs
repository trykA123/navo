pub(super) fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) const END: u8 = 0;
pub(super) const WORD: u8 = 1;

/// Byte kinds for [`word_len`]: 0 ends a word, 1 is an ASCII word byte, 2 starts a non-ASCII char.
pub(super) const WORD_BYTES: [u8; 256] = {
    let mut table = [0_u8; 256];
    let mut byte = 0;
    while byte < 256 {
        let ascii = byte < 128;
        table[byte] = if !ascii {
            2
        } else if (byte as u8).is_ascii_alphanumeric() || byte == b'_' as usize {
            WORD
        } else {
            END
        };
        byte += 1;
    }
    table
};

/// Returns the byte length of the word at the start of `text`; ASCII bytes use a table lookup.
pub(super) fn word_len(text: &str) -> usize {
    for (end, &byte) in text.as_bytes().iter().enumerate() {
        match WORD_BYTES[usize::from(byte)] {
            WORD => {}
            END => return end,
            _ => {
                let tail = text.get(end..).unwrap_or_default();
                return end + tail.find(|ch: char| !is_word(ch)).unwrap_or(tail.len());
            }
        }
    }
    text.len()
}

/// True when the character just before byte offset `at` is a word character.
pub(super) fn word_before(text: &str, at: usize) -> bool {
    match at.checked_sub(1).and_then(|last| text.as_bytes().get(last)) {
        None => false,
        Some(byte) if byte.is_ascii() => byte.is_ascii_alphanumeric() || *byte == b'_',
        Some(_) => text
            .get(..at)
            .and_then(|head| head.chars().next_back())
            .is_some_and(is_word),
    }
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
