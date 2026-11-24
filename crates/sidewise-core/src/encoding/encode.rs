//! Encodes editor text back into bytes for a save, refusing any character the target would
//! not read back unchanged (an unmappable one, or a WHATWG compatibility mapping such as ¥).

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use super::{EncodeError, TextEncoding};

/// Encodes `text` as `target`, with its BOM. Fails with `Unmappable` on the first character a
/// legacy target cannot hold or would read back as another character.
pub(super) fn encode(text: &str, target: &TextEncoding) -> Result<Vec<u8>, EncodeError> {
    let encoding = target.encoding;
    let mut bytes = Vec::with_capacity(text.len() + 3);
    if encoding == UTF_16LE || encoding == UTF_16BE {
        let little = encoding == UTF_16LE;
        if target.bom {
            bytes.extend_from_slice(if little { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
        }
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        return Ok(bytes);
    }
    if encoding == UTF_8 {
        if target.bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        bytes.extend_from_slice(text.as_bytes());
        return Ok(bytes);
    }
    let (encoded, _, unmappable) = encoding.encode(text);
    if unmappable {
        return Err(first_unmappable(text, encoding));
    }
    if let Some(changed) = first_changed(text, &encoded, encoding) {
        return Err(changed);
    }
    bytes.extend_from_slice(&encoded);
    Ok(bytes)
}

/// The first character of `text` that `encoded` decodes back differently in `encoding`, with
/// its 1-based line, or None when the round trip is exact. One pass over both texts.
fn first_changed(text: &str, encoded: &[u8], encoding: &'static Encoding) -> Option<EncodeError> {
    let (back, _) = encoding.decode_without_bom_handling(encoded);
    let mut back = back.chars();
    let mut line = 1;
    for character in text.chars() {
        if back.next() != Some(character) {
            return Some(EncodeError::Unmappable { line, character });
        }
        if character == '\n' {
            line += 1;
        }
    }
    // Fail closed: extra decoded characters also mean the file would not read back as `text`.
    back.next().map(|_| EncodeError::Unmappable {
        line,
        character: '\u{FFFD}',
    })
}

/// The first character of `text` that `encoding` cannot hold, with its 1-based line.
fn first_unmappable(text: &str, encoding: &'static Encoding) -> EncodeError {
    let mut buffer = [0; 4];
    let found = text.split('\n').enumerate().find_map(|(index, line)| {
        line.chars()
            .find(|character| encoding.encode(character.encode_utf8(&mut buffer)).2)
            .map(|character| (index as u64 + 1, character))
    });
    let (line, character) = found.unwrap_or((1, '\u{FFFD}'));
    EncodeError::Unmappable { line, character }
}
