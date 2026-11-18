//! Log words, numbers and level roles, plus the byte-class table; used by log.rs.

/// Byte class: plain bytes are skipped without any token attempt.
pub(super) const PLAIN: u8 = 0;
/// Byte class: ASCII letter, digit or `_`.
pub(super) const WORD: u8 = 1;
/// Byte class: a byte that may open a shape or number but is no word byte.
pub(super) const OPENER: u8 = 2;
/// Byte class: `"`.
pub(super) const DOUBLE: u8 = 3;
/// Byte class: `'`.
pub(super) const SINGLE: u8 = 4;

/// Maps each byte to its class, so plain runs are skipped by one table load per byte.
pub(super) const CLASS: [u8; 256] = build_class();

/// Builds the class table at compile time.
const fn build_class() -> [u8; 256] {
    let mut table = [PLAIN; 256];
    let mut byte = 0;
    while byte < 256 {
        let b = byte as u8;
        table[byte] = if b.is_ascii_alphanumeric() || b == b'_' {
            WORD
        } else if matches!(b, b'-' | b'/' | b'\\' | b'{') {
            OPENER
        } else if b == b'"' {
            DOUBLE
        } else if b == b'\'' {
            SINGLE
        } else {
            PLAIN
        };
        byte += 1;
    }
    table
}

/// Returns true for the bytes that form a Log word: ASCII alphanumeric and `_`.
pub(super) fn is_word(byte: u8) -> bool {
    CLASS[byte as usize] == WORD
}

/// Returns the end of the run of word bytes that starts at `at`.
pub(super) fn run_end(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && is_word(bytes[at]) {
        at += 1;
    }
    at
}

/// Returns the end of a dotted word whose first word-byte run ends at `run`.
///
/// The chunk end counts as a boundary: a dot at the very end stays outside the word.
pub(super) fn word_end(bytes: &[u8], mut run: usize) -> usize {
    while let (Some(b'.'), Some(&next)) = (bytes.get(run), bytes.get(run + 1)) {
        if !is_word(next) {
            break;
        }
        run = run_end(bytes, run + 1);
    }
    run
}

/// Returns the end of a `0x` hex number or a bare 8/16-digit hex number in the run `at..run`.
fn hex_end(bytes: &[u8], at: usize, run: usize) -> Option<usize> {
    let word = bytes.get(at..run)?;
    if word.len() > 2 && word[0] == b'0' && matches!(word[1], b'x' | b'X') {
        return word[2..].iter().all(u8::is_ascii_hexdigit).then_some(run);
    }
    if !matches!(word.len(), 8 | 16) || !word.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let digit = word.iter().any(u8::is_ascii_digit);
    let alpha = word.iter().any(u8::is_ascii_alphabetic);
    (digit && alpha).then_some(run)
}

/// Returns the end of a decimal or hex number at `at`, or None when none starts here.
///
/// `run` is the end of the word-byte run that starts at `at` (equal to `at` for a `-`).
pub(super) fn number_end(bytes: &[u8], at: usize, run: usize) -> Option<usize> {
    let lead = bytes[at];
    if lead != b'-'
        && let Some(end) = hex_end(bytes, at, run)
    {
        return Some(end);
    }
    let first = if lead == b'-' { at + 1 } else { at };
    if !bytes.get(first).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut end = first;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    while bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    end = exponent_end(bytes, end);
    boundary(bytes, end).then_some(end)
}

/// Returns the end past an optional `e[+-]digits` exponent at `end`, or `end`.
fn exponent_end(bytes: &[u8], end: usize) -> usize {
    if !bytes.get(end).is_some_and(|&b| b == b'e' || b == b'E') {
        return end;
    }
    let mut next = end + 1;
    if bytes.get(next).is_some_and(|&b| b == b'+' || b == b'-') {
        next += 1;
    }
    if !bytes.get(next).is_some_and(u8::is_ascii_digit) {
        return end;
    }
    while bytes.get(next).is_some_and(u8::is_ascii_digit) {
        next += 1;
    }
    next
}

/// Returns true when `at` may end a token: the chunk end or a non-word byte.
fn boundary(bytes: &[u8], at: usize) -> bool {
    bytes.get(at).is_none_or(|&b| !is_word(b))
}

/// Returns the D5 role of a whole level word, or None when it stays plain.
pub(super) fn level(word: &str, next: Option<u8>) -> Option<&'static str> {
    if !(3..=9).contains(&word.len()) {
        return None;
    }
    if word.as_bytes()[0].is_ascii_uppercase()
        && let Some(role) = upper_role(word)
    {
        return Some(role);
    }
    if matches!(next, Some(b':' | b']' | b'|')) && is_lower_or_capitalised(word) {
        return lower_role(word);
    }
    None
}

/// Returns the role of an upper-case level word.
fn upper_role(word: &str) -> Option<&'static str> {
    match word {
        "ERROR" | "ERR" | "FATAL" | "CRITICAL" | "CRIT" | "FAIL" | "FAILED" | "FAILURE"
        | "PANIC" | "EXCEPTION" => Some("title"),
        "WARN" | "WARNING" => Some("keyword"),
        "INFO" | "NOTICE" => Some("function"),
        "DEBUG" | "TRACE" | "VERBOSE" | "DBG" => Some("comment.doc"),
        _ => None,
    }
}

/// Returns the role of a lower-case or Capitalised level word.
fn lower_role(word: &str) -> Option<&'static str> {
    let mut upper = [0u8; 16];
    let bytes = word.as_bytes();
    for (slot, byte) in upper.iter_mut().zip(bytes) {
        *slot = byte.to_ascii_uppercase();
    }
    let upper = std::str::from_utf8(&upper[..bytes.len()]).ok()?;
    upper_role(upper)
}

/// Returns true for an all-lower-case word or one with a single leading capital.
fn is_lower_or_capitalised(word: &str) -> bool {
    let b = word.as_bytes();
    b.first().is_some_and(u8::is_ascii_alphabetic) && b[1..].iter().all(u8::is_ascii_lowercase)
}
