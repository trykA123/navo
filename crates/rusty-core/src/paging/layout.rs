use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE};
use memchr::{Memchr, memchr_iter, memrchr};

use super::ByteSource;
use crate::encoding::TextEncoding;

const CR: u8 = b'\r';
const LF: u8 = b'\n';
const CONTINUATION_STEPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextLayout {
    pub text: TextEncoding,
}

impl TextLayout {
    #[must_use]
    pub fn of(text: TextEncoding) -> Self {
        Self { text }
    }

    #[must_use]
    pub fn is_utf8(&self) -> bool {
        self.text.encoding == UTF_8
    }

    #[must_use]
    pub fn is_wide(&self) -> bool {
        self.text.encoding == UTF_16LE || self.text.encoding == UTF_16BE
    }

    #[must_use]
    pub fn unit(&self) -> usize {
        if self.is_wide() { 2 } else { 1 }
    }

    #[must_use]
    pub fn align(&self, offset: u64) -> u64 {
        if self.is_wide() { offset & !1 } else { offset }
    }

    #[must_use]
    pub fn bom_len(&self) -> usize {
        match (self.text.bom, self.is_wide()) {
            (false, _) => 0,
            (true, true) => 2,
            (true, false) => 3,
        }
    }

    fn is_unit(&self, bytes: &[u8], ascii: u8) -> bool {
        match (self.is_wide(), self.text.encoding == UTF_16BE) {
            (false, _) => bytes == [ascii],
            (true, false) => bytes == [ascii, 0],
            (true, true) => bytes == [0, ascii],
        }
    }

    #[must_use]
    pub fn is_lf(&self, unit: &[u8]) -> bool {
        self.is_unit(unit, LF)
    }

    #[must_use]
    pub fn is_cr(&self, unit: &[u8]) -> bool {
        self.is_unit(unit, CR)
    }

    #[must_use]
    pub fn newlines<'a>(&self, hay: &'a [u8]) -> Newlines<'a> {
        Newlines {
            layout: *self,
            hay,
            found: memchr_iter(LF, hay),
        }
    }

    #[must_use]
    pub fn first_newline(&self, hay: &[u8]) -> Option<usize> {
        self.newlines(hay).next()
    }

    #[must_use]
    pub fn last_newline(&self, hay: &[u8]) -> Option<usize> {
        if !self.is_wide() {
            return memrchr(LF, hay);
        }
        let mut end = hay.len();
        while let Some(at) = memrchr(LF, &hay[..end]) {
            if let Some(start) = self.unit_start(hay, at) {
                return Some(start);
            }
            end = at;
        }
        None
    }

    fn unit_start(&self, hay: &[u8], lf_byte: usize) -> Option<usize> {
        if !self.is_wide() {
            return Some(lf_byte);
        }
        let start = if self.text.encoding == UTF_16BE {
            lf_byte.checked_sub(1)?
        } else {
            lf_byte
        };
        let unit = hay.get(start..start + 2)?;
        (start % 2 == 0 && self.is_lf(unit)).then_some(start)
    }

    #[must_use]
    pub fn cut_back(&self, read: &[u8], cut: usize) -> usize {
        if self.is_wide() {
            let low_surrogate = read
                .get(cut..cut + 2)
                .is_some_and(|unit| (0xDC00..0xE000).contains(&self.unit_value(unit)));
            return if low_surrogate { cut - 2 } else { cut };
        }
        if !self.is_utf8() {
            return cut;
        }
        let mut cut = cut;
        for _ in 0..CONTINUATION_STEPS {
            if read[cut] & 0xC0 != 0x80 {
                break;
            }
            cut -= 1;
        }
        cut
    }

    #[must_use]
    pub fn skip_continuation(&self, lead: &[u8]) -> usize {
        if self.is_wide() {
            let low_surrogate = lead
                .get(..2)
                .is_some_and(|unit| (0xDC00..0xE000).contains(&self.unit_value(unit)));
            return if low_surrogate { 2 } else { 0 };
        }
        if !self.is_utf8() {
            return 0;
        }
        lead.iter()
            .take(CONTINUATION_STEPS)
            .take_while(|byte| **byte & 0xC0 == 0x80)
            .count()
    }

    fn unit_value(&self, unit: &[u8]) -> u16 {
        let pair = [unit[0], unit[1]];
        if self.text.encoding == UTF_16BE {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    }

    #[must_use]
    pub fn decode(&self, bytes: &[u8]) -> String {
        if self.is_utf8() {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let (text, _) = self.text.encoding.decode_without_bom_handling(bytes);
        text.into_owned()
    }

    #[must_use]
    pub fn raw_len(&self, character: char) -> usize {
        if self.is_utf8() {
            character.len_utf8()
        } else if self.is_wide() {
            character.len_utf16() * 2
        } else if self.text.encoding.is_single_byte() {
            1
        } else {
            let mut buffer = [0; 4];
            let (bytes, _, _) = self
                .text
                .encoding
                .encode(character.encode_utf8(&mut buffer));
            bytes.len()
        }
    }

    #[must_use]
    pub fn text_range(&self, bytes: &[u8], from: usize, width: usize) -> std::ops::Range<usize> {
        if self.is_utf8() {
            return from..from + width;
        }
        let from = from.min(bytes.len());
        let to = (from + width).min(bytes.len());
        let start = self.decode(&bytes[..from]).len();
        start..start + self.decode(&bytes[from..to]).len()
    }

    #[must_use]
    pub fn encode(&self, text: &str) -> Option<Vec<u8>> {
        if self.is_utf8() {
            return Some(text.as_bytes().to_vec());
        }
        if self.is_wide() {
            let little = self.text.encoding == UTF_16LE;
            return Some(
                text.encode_utf16()
                    .flat_map(|unit| unit_bytes(unit, little))
                    .collect(),
            );
        }
        let (bytes, _, unmappable) = self.text.encoding.encode(text);
        (!unmappable).then(|| bytes.into_owned())
    }
}

fn unit_bytes(unit: u16, little: bool) -> [u8; 2] {
    if little {
        unit.to_le_bytes()
    } else {
        unit.to_be_bytes()
    }
}

pub struct Newlines<'a> {
    layout: TextLayout,
    hay: &'a [u8],
    found: Memchr<'a>,
}

impl Iterator for Newlines<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        self.found
            .find_map(|at| self.layout.unit_start(self.hay, at))
    }
}

pub struct Laid<S> {
    inner: S,
    layout: TextLayout,
}

impl<S> Laid<S> {
    #[must_use]
    pub fn new(inner: S, layout: TextLayout) -> Self {
        Self { inner, layout }
    }
}

impl<S: ByteSource> ByteSource for Laid<S> {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.inner.read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        self.inner.take_changed()
    }

    fn take_rewritten(&self) -> bool {
        self.inner.take_rewritten()
    }

    fn layout(&self) -> TextLayout {
        self.layout
    }
}
