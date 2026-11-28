//! Line decoding, explicit `enc:` decoders and UTF-16 detection for the find matcher.

use std::borrow::Cow;

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::super::TextEncoding;

/// Maximum number of characters kept in a reported hit snippet.
const TEXT_LIMIT: usize = 200;

/// CP437 glyphs for bytes `0x80..=0xFF`; ASCII bytes map to themselves.
const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Detects a UTF-16 byte-order mark at the start of the probed file head.
fn bom_from_head(head: &[u8]) -> Option<&'static Encoding> {
    match head.get(..2) {
        Some([0xff, 0xfe]) => Some(UTF_16LE),
        Some([0xfe, 0xff]) => Some(UTF_16BE),
        _ => None,
    }
}

/// Number of byte-order-mark bytes to skip at the start of `head`.
pub(super) fn bom_len(head: &[u8]) -> usize {
    if bom_from_head(head).is_some() { 2 } else { 0 }
}

/// Whether at least 30% of byte pairs carry a NUL in the odd position (UTF-16LE text).
fn nul_heavy(head: &[u8]) -> bool {
    let pairs = head.as_chunks::<2>().0;
    let nul = pairs.iter().filter(|pair| pair[1] == 0).count();
    !pairs.is_empty() && nul * 10 >= pairs.len() * 3
}

/// Decodes one raw line, falling back to Windows-1252 for invalid UTF-8.
fn decode_line(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => WINDOWS_1252.decode_without_bom_handling(bytes).0,
    }
}

/// Decodes one raw line as CP437, borrowing ASCII unchanged.
fn dos_decode(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes)
        && text.is_ascii()
    {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        bytes
            .iter()
            .map(|&byte| {
                if byte < 0x80 {
                    char::from(byte)
                } else {
                    CP437[usize::from(byte - 0x80)]
                }
            })
            .collect(),
    )
}

/// One narrow (single-byte or UTF-8) decoding selected by `enc:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Narrow {
    Auto,
    Utf8,
    Ansi,
    Dos,
}

impl Narrow {
    fn decode(self, bytes: &[u8]) -> Cow<'_, str> {
        match self {
            Self::Auto => decode_line(bytes),
            Self::Utf8 => String::from_utf8_lossy(bytes),
            Self::Ansi => WINDOWS_1252.decode_without_bom_handling(bytes).0,
            Self::Dos => dos_decode(bytes),
        }
    }
}

/// The `enc:` decoder selection; an empty or `auto` selection keeps the auto path.
#[derive(Clone, Copy)]
pub(super) struct Decoders {
    narrow: [Narrow; 4],
    len: usize,
    utf16: bool,
}

impl Decoders {
    /// Builds the decoder list from the parsed `enc:` tokens.
    pub(super) fn new(encodings: &[TextEncoding]) -> Self {
        let mut narrow = [Narrow::Auto; 4];
        let mut len = 0;
        let mut utf16 = false;
        for encoding in encodings {
            let kind = match encoding {
                TextEncoding::Auto => Some(Narrow::Auto),
                TextEncoding::Utf8 => Some(Narrow::Utf8),
                TextEncoding::Ansi => Some(Narrow::Ansi),
                TextEncoding::Dos => Some(Narrow::Dos),
                TextEncoding::Utf16 => {
                    utf16 = true;
                    None
                }
            };
            if let Some(kind) = kind
                && !narrow[..len].contains(&kind)
            {
                narrow[len] = kind;
                len += 1;
            }
        }
        if len == 0 {
            len = 1;
        }
        Self { narrow, len, utf16 }
    }

    /// Detects UTF-16 from a BOM, or from the NUL heuristic when `enc:utf16` is set.
    pub(super) fn wide_from_head(&self, head: &[u8]) -> Option<&'static Encoding> {
        if let Some(encoding) = bom_from_head(head) {
            return Some(encoding);
        }
        if self.utf16 && nul_heavy(head) {
            Some(UTF_16LE)
        } else {
            None
        }
    }

    /// Visits one decoded line per selected encoding (wide files decode once).
    pub(super) fn decode(
        self,
        bytes: &[u8],
        wide: Option<&'static Encoding>,
        mut visit: impl FnMut(&str),
    ) {
        if let Some(encoding) = wide {
            visit(&encoding.decode_without_bom_handling(bytes).0);
            return;
        }
        for narrow in &self.narrow[..self.len] {
            visit(&narrow.decode(bytes));
        }
    }
}

/// Returns at most `TEXT_LIMIT` characters of `text` around `column`.
pub(super) fn snippet(text: &str, column: usize) -> String {
    let length = text.chars().count();
    let start = column
        .saturating_sub(TEXT_LIMIT / 2)
        .min(length.saturating_sub(TEXT_LIMIT));
    text.chars().skip(start).take(TEXT_LIMIT).collect()
}

#[cfg(test)]
thread_local! {
    static CHUNK_GROWTHS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Records one chunk-buffer capacity growth (test-only).
#[cfg(test)]
pub(in crate::find) fn note_chunk_growth() {
    CHUNK_GROWTHS.with(|cell| cell.set(cell.get() + 1));
}

/// Resets the chunk-buffer growth counter (test-only).
#[cfg(test)]
pub(in crate::find) fn reset_chunk_growths() {
    CHUNK_GROWTHS.with(|cell| cell.set(0));
}

/// Reads the chunk-buffer growth counter (test-only).
#[cfg(test)]
pub(in crate::find) fn chunk_growths() -> usize {
    CHUNK_GROWTHS.with(|cell| cell.get())
}
