use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use super::{EncodeError, TextEncoding};

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
    bytes.extend_from_slice(&encoded);
    Ok(bytes)
}

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
