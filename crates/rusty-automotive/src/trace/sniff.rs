use super::Base;
use super::asc::{Header, Shape, parse};

const LOOKED_AT: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Header,
    Frames,
}

#[must_use]
pub fn sniff_trace(head: &[u8]) -> Option<Evidence> {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let complete = match head.iter().rposition(|byte| *byte == b'\n') {
        Some(end) => head.get(..end).unwrap_or_default(),
        None => head,
    };
    let (mut frames, mut rows, mut data) = (0usize, 0usize, Vec::new());
    for line in complete.split(|byte| *byte == b'\n').take(LOOKED_AT) {
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
