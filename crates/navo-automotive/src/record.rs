use crate::Format;

pub(crate) const NOT_A_RECORD: &str = "the line does not start a record";
const NOT_HEX: &str = "the record has characters that are not hex digits";
const TOO_SHORT: &str = "the record is too short";
const WRONG_COUNT: &str = "the byte count does not match the record";
const UNKNOWN_TYPE: &str = "unknown record type";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Record<'a> {
    pub kind: u8,
    pub address: u64,
    pub data: &'a [u8],
    pub wrong_sum: Option<(u8, u8)>,
}

fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn decode_hex(text: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str> {
    out.clear();
    let (pairs, rest) = text.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(NOT_HEX);
    }
    for &[high, low] in pairs {
        let (Some(high), Some(low)) = (nibble(high), nibble(low)) else {
            return Err(NOT_HEX);
        };
        out.push(high << 4 | low);
    }
    Ok(())
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |sum, byte| sum.wrapping_add(*byte))
}

fn big_endian(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0, |value, byte| value << 8 | u64::from(*byte))
}

fn checked(stored: u8, computed: u8) -> Option<(u8, u8)> {
    (stored != computed).then_some((stored, computed))
}

fn intel<'a>(line: &[u8], bytes: &'a mut Vec<u8>) -> Result<Record<'a>, &'static str> {
    let digits = line.strip_prefix(b":").ok_or(NOT_A_RECORD)?;
    decode_hex(digits, bytes)?;
    let bytes: &'a [u8] = bytes;
    let (&stored, body) = bytes.split_last().ok_or(TOO_SHORT)?;
    let &[count, high, low, kind, ref data @ ..] = body else {
        return Err(TOO_SHORT);
    };
    if data.len() != usize::from(count) {
        return Err(WRONG_COUNT);
    }
    if kind > 5 {
        return Err(UNKNOWN_TYPE);
    }
    Ok(Record {
        kind,
        address: big_endian(&[high, low]),
        data,
        wrong_sum: checked(stored, 0u8.wrapping_sub(sum(body))),
    })
}

fn address_width(kind: u8) -> Option<usize> {
    match kind {
        0 | 1 | 5 | 9 => Some(2),
        2 | 6 | 8 => Some(3),
        3 | 7 => Some(4),
        _ => None,
    }
}

fn s_record<'a>(line: &[u8], bytes: &'a mut Vec<u8>) -> Result<Record<'a>, &'static str> {
    let &[b'S', kind, ref digits @ ..] = line else {
        return Err(NOT_A_RECORD);
    };
    let kind = kind.checked_sub(b'0').ok_or(NOT_A_RECORD)?;
    let width = address_width(kind).ok_or(if kind <= 9 {
        UNKNOWN_TYPE
    } else {
        NOT_A_RECORD
    })?;
    decode_hex(digits, bytes)?;
    let bytes: &'a [u8] = bytes;
    let (&stored, body) = bytes.split_last().ok_or(TOO_SHORT)?;
    let (&count, rest) = body.split_first().ok_or(TOO_SHORT)?;
    if usize::from(count) != rest.len() + 1 {
        return Err(WRONG_COUNT);
    }
    let (address, data) = rest.split_at_checked(width).ok_or(TOO_SHORT)?;
    Ok(Record {
        kind,
        address: big_endian(address),
        data,
        wrong_sum: checked(stored, !sum(body)),
    })
}

pub(crate) fn decode<'a>(
    format: Format,
    line: &[u8],
    bytes: &'a mut Vec<u8>,
) -> Result<Record<'a>, &'static str> {
    match format {
        Format::IntelHex => intel(line, bytes),
        Format::SRecord => s_record(line, bytes),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Data(u64),
    Start(crate::Start),
    End(Option<crate::Start>),
    Other,
}

const WRONG_LENGTH: &str = "the record has the wrong length for its type";

pub(crate) struct Cursor {
    pub format: Format,
    pub base: u64,
}

impl Cursor {
    pub(crate) fn locate(&mut self, record: &Record<'_>) -> Result<Event, &'static str> {
        use crate::Start;
        let value = |len: usize| {
            (record.data.len() == len)
                .then(|| big_endian(record.data))
                .ok_or(WRONG_LENGTH)
        };
        Ok(match (self.format, record.kind) {
            (Format::IntelHex, 0) => Event::Data(self.base + record.address),
            (Format::IntelHex, 1) => Event::End(None),
            (Format::IntelHex, 2) => {
                self.base = value(2)? << 4;
                Event::Other
            }
            (Format::IntelHex, 4) => {
                self.base = value(2)? << 16;
                Event::Other
            }
            (Format::IntelHex, 3) => {
                let value = value(4)?;
                Event::Start(Start::Segment {
                    cs: u16::try_from(value >> 16).map_err(|_| WRONG_LENGTH)?,
                    ip: u16::try_from(value & 0xFFFF).map_err(|_| WRONG_LENGTH)?,
                })
            }
            (Format::IntelHex, 5) => Event::Start(Start::Linear(value(4)?)),
            (Format::SRecord, 1..=3) => Event::Data(record.address),
            (Format::SRecord, 7..=9) => Event::End(Some(Start::Linear(record.address))),
            _ => Event::Other,
        })
    }
}
