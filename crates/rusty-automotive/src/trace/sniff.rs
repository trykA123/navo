use super::Base;
use super::asc::{Header, Shape, parse};

const LOOKED_AT: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Header,
    Frames,
}

#[must_use]
pub fn sniff_blf(head: &[u8]) -> bool {
    if head.len() < 72 || !head.starts_with(b"LOGG") {
        return false;
    }
    let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    let field = |at: usize| u16::from_le_bytes([head[at], head[at + 1]]);
    (72..=4096).contains(&size)
        && (1980..=2100).contains(&field(40))
        && (1..=12).contains(&field(42))
        && (1..=31).contains(&field(46))
}

#[must_use]
pub fn sniff_trace(head: &[u8]) -> Option<Evidence> {
    if sniff_blf(head) {
        return Some(Evidence::Header);
    }
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let complete = match head.iter().rposition(|byte| *byte == b'\n') {
        Some(end) => head.get(..end).unwrap_or_default(),
        None => head,
    };
    let (mut frames, mut rows, mut data) = (0usize, 0usize, Vec::new());
    for line in complete.split(|byte| *byte == b'\n').take(LOOKED_AT) {
        let line = line.trim_ascii();
        if super::trc::is_header(line) {
            return (!super::trc::has_unsupported_version(line)).then_some(Evidence::Header);
        }
        match parse(line, Base::Hex, &mut data).1 {
            Shape::Header(Header::Notation {
                base: Some(_),
                relative: Some(_),
            }) => return Some(Evidence::Header),
            Shape::Blank | Shape::Header(_) => {}
            Shape::Frame(_) => {
                frames += 1;
                rows += 1;
            }
            Shape::Other | Shape::Malformed(_) => rows += 1,
        }
    }
    (frames > 0 && frames * 2 >= rows).then_some(Evidence::Frames)
}
