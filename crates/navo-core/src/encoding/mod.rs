mod detect;
mod encode;
mod utf16;

use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

pub use encoding_rs::Encoding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedBy {
    Bom,
    Declaration,
    Utf8,
    Guess,
    Fallback,
    Chosen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEncoding {
    pub encoding: &'static Encoding,
    pub bom: bool,
    pub by: DetectedBy,
}

impl Default for TextEncoding {
    fn default() -> Self {
        Self {
            encoding: UTF_8,
            bom: false,
            by: DetectedBy::Utf8,
        }
    }
}

#[must_use]
pub fn encoding_name(encoding: &'static Encoding) -> String {
    match encoding {
        e if e == UTF_16LE => "UTF-16 LE".to_owned(),
        e if e == UTF_16BE => "UTF-16 BE".to_owned(),
        e => e
            .name()
            .strip_prefix("windows-")
            .map_or_else(|| e.name().to_owned(), |code| format!("Windows-{code}")),
    }
}

impl TextEncoding {
    #[must_use]
    pub fn label(&self) -> String {
        let name = encoding_name(self.encoding);
        if self.bom {
            format!("{name} BOM")
        } else {
            name
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: TextEncoding,
    pub lossless: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncodeError {
    #[error("'{character}' on line {line} cannot be saved in this encoding")]
    Unmappable { line: u64, character: char },
}

pub const PICKER: &[&str] = &[
    "UTF-8",
    "UTF-16LE",
    "UTF-16BE",
    "windows-1250",
    "windows-1251",
    "windows-1252",
    "windows-1253",
    "windows-1254",
    "windows-1255",
    "windows-1256",
    "windows-1257",
    "windows-1258",
    "ISO-8859-1",
    "ISO-8859-2",
    "ISO-8859-15",
    "KOI8-R",
    "Shift_JIS",
    "EUC-JP",
    "GBK",
    "Big5",
    "EUC-KR",
    "IBM866",
];

#[must_use]
pub fn picker() -> Vec<&'static Encoding> {
    let mut offered: Vec<&'static Encoding> = Vec::new();
    for label in PICKER {
        if let Some(encoding) = Encoding::for_label(label.as_bytes())
            && !offered.contains(&encoding)
        {
            offered.push(encoding);
        }
    }
    offered
}

#[must_use]
pub fn picker_name(encoding: &'static Encoding) -> String {
    if encoding == WINDOWS_1252 {
        return "Windows-1252 · ISO-8859-1".to_owned();
    }
    encoding_name(encoding)
}

#[must_use]
pub fn detect(bytes: &[u8], fallback: &'static Encoding) -> TextEncoding {
    detect::detect(bytes, fallback)
}

#[must_use]
pub fn decode(bytes: &[u8], fallback: &'static Encoding) -> Decoded {
    let encoding = detect(bytes, fallback);
    decode_with(bytes, encoding)
}

#[must_use]
pub fn choose(bytes: &[u8], encoding: &'static Encoding) -> TextEncoding {
    let bom = detect::bom(bytes).is_some_and(|(found, _)| found == encoding);
    detect::found(encoding, bom, DetectedBy::Chosen)
}

#[must_use]
pub fn decode_as(bytes: &[u8], encoding: &'static Encoding) -> Decoded {
    decode_with(bytes, choose(bytes, encoding))
}

#[must_use]
pub fn is_lossy(bytes: &[u8], encoding: &TextEncoding) -> bool {
    let skip = if encoding.bom {
        detect::bom(bytes).map_or(0, |(_, len)| len)
    } else {
        0
    };
    encoding
        .encoding
        .decode_without_bom_handling(&bytes[skip..])
        .1
}

fn decode_with(bytes: &[u8], encoding: TextEncoding) -> Decoded {
    let skip = if encoding.bom {
        detect::bom(bytes).map_or(0, |(_, len)| len)
    } else {
        0
    };
    let body = &bytes[skip..];
    let (text, malformed) = encoding.encoding.decode_without_bom_handling(body);
    let text = text.into_owned();
    let lossless = !malformed
        && !text.contains('\0')
        && encode(&text, &encoding).is_ok_and(|again| again == bytes);
    Decoded {
        text,
        encoding,
        lossless,
    }
}

pub fn encode(text: &str, encoding: &TextEncoding) -> Result<Vec<u8>, EncodeError> {
    encode::encode(text, encoding)
}

#[must_use]
pub fn is_utf16(head: &[u8]) -> bool {
    detect::bom(head).is_some_and(|(found, _)| utf16::is_utf16(found))
        || utf16::by_pattern(head).is_some()
}

#[must_use]
pub fn looks_binary(head: &[u8]) -> bool {
    if let Some((encoding, skip)) = detect::bom(head).filter(|(found, _)| utf16::is_utf16(found)) {
        return !utf16::plausible(&head[skip..], encoding);
    }
    utf16::by_pattern(head).is_none() && head.contains(&0)
}

#[cfg(test)]
mod tests;
