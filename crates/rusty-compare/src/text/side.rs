use std::io::{ErrorKind, Read};

use rusty_core::encoding::{Encoding, decode_as, detect as detect_bytes, looks_binary};
use rusty_core::paging::TextLayout;

use crate::model::{ContentClass, ReadHint, Reason, SideEntry, SideSource};

use super::normalise::{Options, normalise};

pub const SNIFF_LEN: usize = 64 * 1024;
pub const BINARY_SNIFF: usize = 8_000;
pub const WHOLE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSide {
    Whole(Whole),
    Large(Large),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whole {
    pub layout: TextLayout,
    pub lines: Vec<Line>,
    pub lossless: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Large {
    pub layout: TextLayout,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub ending: LineEnding,
    pub bom: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    Lf,
    CrLf,
    None,
}

impl LineEnding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::None => "",
        }
    }
}

impl TextSide {
    pub fn decode(
        source: &dyn SideSource,
        entry: &SideEntry,
        fallback: &'static Encoding,
    ) -> Result<Self, Reason> {
        let mut reader = source.open(&entry.rel, ReadHint::Buffered)?;
        let mut head = vec![0u8; SNIFF_LEN];
        let filled = fill(reader.as_mut(), &mut head)?;
        head.truncate(filled);
        if content_class(&head) == ContentClass::Binary {
            return Err(Reason::Unsupported);
        }
        let layout = detect(&head, fallback);
        if entry.size > WHOLE_LIMIT {
            return Ok(Self::Large(Large {
                layout,
                len: entry.size,
            }));
        }
        let mut bytes = head;
        let limit = WHOLE_LIMIT
            .saturating_sub(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            .saturating_add(1);
        reader
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|_| Reason::ReadError)?;
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if len > WHOLE_LIMIT {
            return Ok(Self::Large(Large {
                layout,
                len: entry.size.max(len),
            }));
        }
        let decoded = decode_as(&bytes, layout.text.encoding);
        Ok(Self::Whole(Whole {
            layout,
            lines: split_lines(&decoded.text, layout.text.bom),
            lossless: decoded.lossless,
        }))
    }
}

impl Whole {
    #[must_use]
    pub fn keys(&self, options: &Options) -> Vec<String> {
        if self.lines.is_empty() && self.layout.text.bom && !options.ignore_bom {
            return vec![normalise("", LineEnding::None, true, options)];
        }
        self.lines
            .iter()
            .map(|line| normalise(&line.text, line.ending, line.bom, options))
            .collect()
    }
}

#[must_use]
pub fn detect(prefix: &[u8], fallback: &'static Encoding) -> TextLayout {
    let head = &prefix[..prefix.len().min(SNIFF_LEN)];
    TextLayout::of(detect_bytes(head, fallback))
}

#[must_use]
pub fn content_class(prefix: &[u8]) -> ContentClass {
    let head = &prefix[..prefix.len().min(BINARY_SNIFF)];
    if looks_binary(head) {
        ContentClass::Binary
    } else {
        ContentClass::Text
    }
}

fn fill(reader: &mut dyn Read, buf: &mut [u8]) -> Result<usize, Reason> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(Reason::ReadError),
        }
    }
    Ok(filled)
}

fn split_lines(text: &str, bom: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    for (index, segment) in text.split_inclusive('\n').enumerate() {
        let (body, ending) = match segment.strip_suffix('\n') {
            Some(rest) => match rest.strip_suffix('\r') {
                Some(body) => (body, LineEnding::CrLf),
                None => (rest, LineEnding::Lf),
            },
            None => (segment, LineEnding::None),
        };
        lines.push(Line {
            text: body.to_owned(),
            ending,
            bom: bom && index == 0,
        });
    }
    lines
}

#[cfg(test)]
mod tests;
