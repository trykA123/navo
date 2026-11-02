use encoding_rs::{Encoding, UTF_16BE, UTF_16LE};

const SAMPLE: usize = 4096;

pub(super) fn is_utf32_le(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xFE, 0, 0])
}

pub(super) fn is_utf16(encoding: &'static Encoding) -> bool {
    encoding == UTF_16LE || encoding == UTF_16BE
}

pub(super) fn by_pattern(bytes: &[u8]) -> Option<&'static Encoding> {
    let sample = &bytes[..bytes.len().min(SAMPLE) & !1];
    let pairs = sample.len() / 2;
    if pairs < 4 {
        return None;
    }
    let zeros = |offset: usize| {
        sample
            .iter()
            .skip(offset)
            .step_by(2)
            .filter(|b| **b == 0)
            .count()
    };
    let (even, odd) = (zeros(0), zeros(1));
    let mostly = |count: usize| count * 10 >= pairs * 7;
    let rarely = |count: usize| count * 20 <= pairs;
    let encoding = if mostly(odd) && rarely(even) {
        UTF_16LE
    } else if mostly(even) && rarely(odd) {
        UTF_16BE
    } else {
        return None;
    };
    Some(encoding).filter(|encoding| plausible(sample, encoding))
}

pub(super) fn plausible(body: &[u8], encoding: &'static Encoding) -> bool {
    let little = encoding == UTF_16LE;
    let mut units = 0_usize;
    let mut controls = 0_usize;
    for pair in body.as_chunks::<2>().0 {
        let unit = if little {
            u16::from_le_bytes(*pair)
        } else {
            u16::from_be_bytes(*pair)
        };
        if unit == 0 {
            return false;
        }
        units += 1;
        controls += usize::from(unit < 0x20 && !matches!(unit, 0x09 | 0x0A | 0x0D));
    }
    controls * 2 <= units
}
