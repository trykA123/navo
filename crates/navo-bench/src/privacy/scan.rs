use aho_corasick::{AhoCorasick, AhoCorasickBuilder};

use super::{ErrorCode, PrivacyError};

const MAX_NEEDLES: usize = 200_000;
const MAX_PATTERN_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct LeakScanner {
    matcher: AhoCorasick,
}

impl std::fmt::Debug for LeakScanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeakScanner").finish_non_exhaustive()
    }
}

impl LeakScanner {
    /// Protects supplied names, path components, and the current Windows identity.
    pub fn new(planted: &[&str]) -> Result<Self, PrivacyError> {
        let identity: Vec<_> = ["USERNAME", "COMPUTERNAME", "USERPROFILE"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        Self::with_identity(planted, &identity)
    }

    fn with_identity(planted: &[&str], identity: &[String]) -> Result<Self, PrivacyError> {
        let mut patterns = Vec::new();
        let mut budget = 0;
        for needle in planted {
            add_path(&mut patterns, &mut budget, needle)?;
        }
        for value in identity {
            add_path(&mut patterns, &mut budget, value)?;
        }
        let matcher = AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .build(patterns)
            .map_err(|_| PrivacyError::output(ErrorCode::ScanBuild))?;
        Ok(Self { matcher })
    }

    pub fn count_leaks(&self, bytes: &[u8]) -> usize {
        let mut count = self.matcher.find_iter(bytes).count();
        let mut current = bytes.to_vec();
        for _ in 0..8 {
            let percent = decode_percent(&current);
            count = count.max(self.matcher.find_iter(&percent).count());
            let decoded = decode_json(&percent);
            count = count.max(self.matcher.find_iter(&decoded).count());
            let unescaped = decode_html(&decoded);
            count = count.max(self.matcher.find_iter(&unescaped).count());
            if unescaped == current {
                return count;
            }
            current = unescaped;
        }
        // Unresolved escaping fails closed after bounded decoding work.
        count.max(1)
    }
}

/// Returns only a count, never a matching confidential value.
pub fn assert_no_leaks(bytes: &[u8], planted: &[&str]) -> Result<(), PrivacyError> {
    if LeakScanner::new(planted)?.count_leaks(bytes) == 0 {
        Ok(())
    } else {
        Err(PrivacyError::output(ErrorCode::Redacted))
    }
}

fn add_path(
    patterns: &mut Vec<Vec<u8>>,
    budget: &mut usize,
    value: &str,
) -> Result<(), PrivacyError> {
    add_needle(patterns, budget, value)?;
    if value.contains(['/', '\\']) {
        // A drive letter names no one, and `C:` alone would match CSS such as `numeric:`.
        let parts = value.split(['/', '\\']).filter(|part| !is_drive(part));
        for component in parts {
            add_needle(patterns, budget, component)?;
        }
    }
    Ok(())
}

fn is_drive(part: &str) -> bool {
    matches!(part.as_bytes(), [letter, b':'] if letter.is_ascii_alphabetic())
}

fn add_needle(
    patterns: &mut Vec<Vec<u8>>,
    budget: &mut usize,
    value: &str,
) -> Result<(), PrivacyError> {
    if value.is_empty() {
        return Ok(());
    }
    let cost = value
        .len()
        .checked_mul(5)
        .and_then(|cost| budget.checked_add(cost));
    let Some(cost) = cost.filter(|cost| *cost <= MAX_PATTERN_BYTES) else {
        return Err(PrivacyError::output(ErrorCode::ScanCapacity));
    };
    if patterns.len() / 3 >= MAX_NEEDLES {
        return Err(PrivacyError::output(ErrorCode::ScanCapacity));
    }
    *budget = cost;
    patterns.push(value.as_bytes().to_vec());
    patterns.push(value.encode_utf16().flat_map(u16::to_le_bytes).collect());
    patterns.push(value.encode_utf16().flat_map(u16::to_be_bytes).collect());
    Ok(())
}

fn decode_percent(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(value) = hex(bytes.get(index + 1..index + 3))
        {
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    decoded
}

fn hex(bytes: Option<&[u8]>) -> Option<u8> {
    let bytes = bytes?;
    let value = std::str::from_utf8(bytes).ok()?;
    u8::from_str_radix(value, 16).ok()
}

fn unicode_escape(bytes: &[u8]) -> Option<(char, usize)> {
    let digits = bytes.strip_prefix(b"\\u")?.get(..4)?;
    let first = u16::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
    if (0xd800..=0xdbff).contains(&first) {
        let rest = bytes.get(6..)?.strip_prefix(b"\\u")?.get(..4)?;
        let second = u16::from_str_radix(std::str::from_utf8(rest).ok()?, 16).ok()?;
        return char::decode_utf16([first, second])
            .next()?
            .ok()
            .map(|ch| (ch, 12));
    }
    char::from_u32(u32::from(first)).map(|ch| (ch, 6))
}

fn decode_json(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && let Some((ch, length)) = unicode_escape(&bytes[index..])
        {
            decoded.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            index += length;
        } else if bytes[index] == b'\\'
            && let Some(byte) = bytes.get(index + 1).and_then(|b| json_byte(*b))
        {
            decoded.push(byte);
            index += 2;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    decoded
}

const ENTITIES: [(&[u8], u8); 6] = [
    (b"&amp;", b'&'),
    (b"&lt;", b'<'),
    (b"&gt;", b'>'),
    (b"&quot;", b'"'),
    (b"&apos;", b'\''),
    (b"&nbsp;", b' '),
];

/// Longest numeric reference worth decoding: `&#x10FFFF;` is 10 bytes.
const MAX_NUMERIC_ENTITY: usize = 10;

fn decode_html(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&'
            && let Some((ch, length)) = html_entity(&bytes[index..])
        {
            decoded.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            index += length;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    decoded
}

/// Named entities match in any case; numeric ones are decimal or hexadecimal.
fn html_entity(bytes: &[u8]) -> Option<(char, usize)> {
    for (name, byte) in ENTITIES {
        if bytes.len() >= name.len() && bytes[..name.len()].eq_ignore_ascii_case(name) {
            return Some((char::from(byte), name.len()));
        }
    }
    let digits = bytes.strip_prefix(b"&#")?;
    let end = digits
        .iter()
        .take(MAX_NUMERIC_ENTITY)
        .position(|b| *b == b';')?;
    let body = std::str::from_utf8(&digits[..end]).ok()?;
    let value = match body.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => body.parse().ok()?,
    };
    Some((char::from_u32(value)?, end + 3))
}

fn json_byte(byte: u8) -> Option<u8> {
    match byte {
        b'\\' | b'"' | b'/' => Some(byte),
        b'b' => Some(8),
        b'f' => Some(12),
        b'n' => Some(b'\n'),
        b'r' => Some(b'\r'),
        b't' => Some(b'\t'),
        _ => None,
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
