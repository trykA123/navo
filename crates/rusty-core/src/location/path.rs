use std::borrow::Cow;

// Unpaired surrogates and invalid UTF-8 bytes are written as characters of plane 16.
const WIDE_BASE: u32 = 0x10_0000;
const WIDE_LAST: u32 = WIDE_BASE + 0x7FF;
const BYTE_BASE: u32 = 0x10_0800;
const BYTE_FIRST: u32 = BYTE_BASE + 0x80;
const BYTE_LAST: u32 = BYTE_BASE + 0xFF;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderPath(Repr);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Repr {
    Text(String),
    Wide(Box<[u16]>),
    Bytes(Box<[u8]>),
}

enum Escape {
    Wide(u16),
    Byte(u8),
}

fn escape_of(c: char) -> Option<Escape> {
    let code = u32::from(c);
    match code {
        WIDE_BASE..=WIDE_LAST => u16::try_from(code - WIDE_BASE + 0xD800)
            .ok()
            .map(Escape::Wide),
        BYTE_FIRST..=BYTE_LAST => u8::try_from(code - BYTE_BASE).ok().map(Escape::Byte),
        _ => None,
    }
}

impl ProviderPath {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let escapes: Vec<Escape> = text.chars().filter_map(escape_of).collect();
        let wide = escapes.iter().any(|e| matches!(e, Escape::Wide(_)));
        let bytes = escapes.iter().any(|e| matches!(e, Escape::Byte(_)));
        match (wide, bytes) {
            (true, false) => Self::from_wide(&decode_wide(text)),
            (false, true) => Self::from_bytes(&decode_bytes(text)),
            _ => Self(Repr::Text(text.to_owned())),
        }
    }

    #[must_use]
    pub fn from_wide(units: &[u16]) -> Self {
        match String::from_utf16(units) {
            Ok(text) => Self(Repr::Text(text)),
            Err(_) => Self(Repr::Wide(units.into())),
        }
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self(Repr::Text(text.to_owned())),
            Err(_) => Self(Repr::Bytes(bytes.into())),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Repr::Text(text) => Some(text),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        match &self.0 {
            Repr::Text(text) => text.clone(),
            Repr::Wide(units) => char::decode_utf16(units.iter().copied())
                .map(|r| r.unwrap_or_else(|e| wide_escape(e.unpaired_surrogate())))
                .collect(),
            Repr::Bytes(bytes) => escape_bytes(bytes),
        }
    }

    #[must_use]
    pub fn to_wide(&self) -> Option<Vec<u16>> {
        match &self.0 {
            Repr::Text(text) => Some(text.encode_utf16().collect()),
            Repr::Wide(units) => Some(units.to_vec()),
            Repr::Bytes(_) => None,
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        match &self.0 {
            Repr::Text(text) => Some(text.as_bytes().to_vec()),
            Repr::Bytes(bytes) => Some(bytes.to_vec()),
            Repr::Wide(_) => None,
        }
    }

    #[must_use]
    pub fn display(&self) -> Cow<'_, str> {
        match &self.0 {
            Repr::Text(text) => Cow::Borrowed(text),
            Repr::Wide(units) => Cow::Owned(String::from_utf16_lossy(units)),
            Repr::Bytes(bytes) => String::from_utf8_lossy(bytes),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        match &self.0 {
            Repr::Text(text) => text.is_empty(),
            Repr::Wide(units) => units.is_empty(),
            Repr::Bytes(bytes) => bytes.is_empty(),
        }
    }

    #[must_use]
    pub fn windows_eq(&self, other: &Self) -> bool {
        match (self.to_wide(), other.to_wide()) {
            (Some(a), Some(b)) => {
                a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| upcase(*x) == upcase(*y))
            }
            _ => self == other,
        }
    }

    pub(super) fn contains_ascii(&self, byte: u8) -> bool {
        match &self.0 {
            Repr::Text(text) => text.as_bytes().contains(&byte),
            Repr::Wide(units) => units.contains(&u16::from(byte)),
            Repr::Bytes(bytes) => bytes.contains(&byte),
        }
    }

    pub(super) fn is_dot_name(&self) -> bool {
        matches!(self.as_str(), Some("." | ".."))
    }

    pub(super) fn join(&self, separator: u8, name: &Self) -> Option<Self> {
        let needs_separator = !self.ends_with_ascii(separator);
        if let (Repr::Text(folder), Repr::Text(child)) = (&self.0, &name.0) {
            let mut text = folder.clone();
            if needs_separator {
                text.push(char::from(separator));
            }
            text.push_str(child);
            return Some(Self(Repr::Text(text)));
        }
        if matches!(self.0, Repr::Bytes(_)) || matches!(name.0, Repr::Bytes(_)) {
            let mut bytes = self.to_bytes()?;
            if needs_separator {
                bytes.push(separator);
            }
            bytes.extend(name.to_bytes()?);
            return Some(Self::from_bytes(&bytes));
        }
        let mut units = self.to_wide()?;
        if needs_separator {
            units.push(u16::from(separator));
        }
        units.extend(name.to_wide()?);
        Some(Self::from_wide(&units))
    }

    fn ends_with_ascii(&self, byte: u8) -> bool {
        match &self.0 {
            Repr::Text(text) => text.as_bytes().last() == Some(&byte),
            Repr::Wide(units) => units.last() == Some(&u16::from(byte)),
            Repr::Bytes(bytes) => bytes.last() == Some(&byte),
        }
    }

    pub(super) fn has_nul(&self) -> bool {
        match &self.0 {
            Repr::Text(text) => text.contains('\0'),
            Repr::Wide(units) => units.contains(&0),
            Repr::Bytes(bytes) => bytes.contains(&0),
        }
    }
}

fn wide_escape(unit: u16) -> char {
    char::from_u32(WIDE_BASE + u32::from(unit) - 0xD800).unwrap_or(char::REPLACEMENT_CHARACTER)
}

fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        for byte in chunk.invalid() {
            out.push(
                char::from_u32(BYTE_BASE + u32::from(*byte)).unwrap_or(char::REPLACEMENT_CHARACTER),
            );
        }
    }
    out
}

fn decode_wide(text: &str) -> Vec<u16> {
    let mut units = Vec::with_capacity(text.len());
    let mut buffer = [0; 2];
    for c in text.chars() {
        match escape_of(c) {
            Some(Escape::Wide(unit)) => units.push(unit),
            _ => units.extend_from_slice(c.encode_utf16(&mut buffer)),
        }
    }
    units
}

fn decode_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut buffer = [0; 4];
    for c in text.chars() {
        match escape_of(c) {
            Some(Escape::Byte(byte)) => bytes.push(byte),
            _ => bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes()),
        }
    }
    bytes
}

fn upcase(unit: u16) -> u16 {
    let Some(c) = char::from_u32(u32::from(unit)) else {
        return unit;
    };
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u16::try_from(u32::from(u)).unwrap_or(unit),
        _ => unit,
    }
}
