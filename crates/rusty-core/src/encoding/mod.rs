use encoding_rs::{Encoding, UTF_8};

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

impl TextEncoding {
    #[must_use]
    pub fn label(&self) -> String {
        self.encoding.name().to_owned()
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
pub fn detect(_bytes: &[u8], _fallback: &'static Encoding) -> TextEncoding {
    TextEncoding {
        encoding: UTF_8,
        bom: false,
        by: DetectedBy::Fallback,
    }
}

#[must_use]
pub fn decode(bytes: &[u8], fallback: &'static Encoding) -> Decoded {
    Decoded {
        text: String::from_utf8_lossy(bytes).into_owned(),
        encoding: detect(bytes, fallback),
        lossless: true,
    }
}

#[must_use]
pub fn decode_as(bytes: &[u8], _encoding: &'static Encoding) -> Decoded {
    decode(bytes, UTF_8)
}

pub fn encode(text: &str, _encoding: &TextEncoding) -> Result<Vec<u8>, EncodeError> {
    Ok(text.as_bytes().to_vec())
}

#[must_use]
pub fn looks_binary(head: &[u8]) -> bool {
    head.contains(&0)
}

#[cfg(test)]
mod tests;
