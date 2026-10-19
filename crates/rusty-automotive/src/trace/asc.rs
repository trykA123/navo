use super::{Base, CanId, Direction, FrameKind, Notation};

pub(crate) const BAD_ID: &str = "the ID is not a valid CAN identifier";
const BAD_DLC: &str = "the DLC is missing or not a number";
const BAD_BYTE: &str = "a data byte is missing or not a number";
const BAD_FD: &str = "the CAN FD flags or length are missing or wrong";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Header {
    Date(String),
    Notation {
        base: Option<Base>,
        relative: Option<bool>,
    },
    Events(bool),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Head {
    pub channel: u16,
    pub id: Option<CanId>,
    pub direction: Option<Direction>,
    pub kind: FrameKind,
    pub dlc: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shape {
    Blank,
    Header(Header),
    Frame(Head),
    Other,
    Malformed(&'static str),
}

type Tokens<'a> =
    std::iter::Filter<std::slice::Split<'a, u8, fn(&u8) -> bool>, fn(&&'a [u8]) -> bool>;

fn tokens(line: &[u8]) -> Tokens<'_> {
    let split: fn(&u8) -> bool = u8::is_ascii_whitespace;
    let keep: fn(&&[u8]) -> bool = |token| !token.is_empty();
    line.split(split).filter(keep)
}

fn starts_with(line: &[u8], prefix: &[u8]) -> bool {
    line.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn header(line: &[u8]) -> Option<Header> {
    if starts_with(line, b"date ") {
        let rest = line.get(5..).unwrap_or_default().trim_ascii();
        return Some(Header::Date(String::from_utf8_lossy(rest).into_owned()));
    }
    if starts_with(line, b"base ") {
        return Some(notation(line));
    }
    if starts_with(line, b"internal events logged") {
        return Some(Header::Events(true));
    }
    if starts_with(line, b"no internal events logged") {
        return Some(Header::Events(false));
    }
    let other = [b"begin triggerblock".as_slice(), b"end triggerblock", b"//"]
        .iter()
        .any(|prefix| starts_with(line, prefix));
    other.then_some(Header::Other)
}

fn notation(line: &[u8]) -> Header {
    let mut base = None;
    let mut relative = None;
    let mut words = tokens(line).peekable();
    while let Some(word) = words.next() {
        let value = words.peek().map(|value| value.to_ascii_lowercase());
        match (word.to_ascii_lowercase().as_slice(), value.as_deref()) {
            (b"base", Some(b"hex")) => base = Some(Base::Hex),
            (b"base", Some(b"dec")) => base = Some(Base::Dec),
            (b"timestamps", Some(b"relative")) => relative = Some(true),
            (b"timestamps", Some(b"absolute")) => relative = Some(false),
            _ => {}
        }
    }
    Header::Notation { base, relative }
}

pub(crate) fn parse_time(token: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(token).ok()?;
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) {
        return None;
    }
    let seconds: u64 = whole.parse().ok()?;
    let mut nanos = 0u64;
    for (at, byte) in fraction.bytes().take(9).enumerate() {
        nanos += u64::from(byte - b'0') * 10u64.pow(8 - at as u32);
    }
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

fn radix(base: Base) -> u32 {
    match base {
        Base::Hex => 16,
        Base::Dec => 10,
    }
}

fn number(token: &[u8], radix: u32) -> Option<u32> {
    let text = std::str::from_utf8(token).ok()?;
    if text.starts_with('+') {
        return None;
    }
    u32::from_str_radix(text, radix).ok()
}

fn small(token: Option<&[u8]>, radix: u32, max: u32) -> Option<u8> {
    let value = number(token?, radix)?;
    u8::try_from(value).ok().filter(|_| value <= max)
}

pub(crate) fn parse_id(token: &[u8], base: Base) -> Option<CanId> {
    let (digits, extended) = match token.split_last() {
        Some((b'x' | b'X', digits)) => (digits, true),
        _ => (token, false),
    };
    let raw = number(digits, radix(base))?;
    let limit = if extended { 0x1FFF_FFFF } else { 0x7FF };
    (raw <= limit).then_some(CanId { raw, extended })
}

fn direction(token: Option<&[u8]>) -> Option<Direction> {
    match token? {
        b"Rx" | b"rx" | b"RX" => Some(Direction::Rx),
        b"Tx" | b"tx" | b"TX" => Some(Direction::Tx),
        b"TxRq" | b"txrq" | b"TXRQ" => Some(Direction::TxRequest),
        _ => None,
    }
}

fn is_error_frame(token: &[u8]) -> bool {
    token.eq_ignore_ascii_case(b"ErrorFrame")
}

fn bytes(words: &mut Tokens<'_>, (count, base): (u8, Base), data: &mut Vec<u8>) -> Option<()> {
    for _ in 0..count {
        data.push(small(words.next(), radix(base), 255)?);
    }
    Some(())
}

fn error_frame(channel: u16, direction: Option<Direction>) -> Shape {
    Shape::Frame(Head {
        channel,
        id: None,
        direction,
        kind: FrameKind::Error,
        dlc: 0,
    })
}

fn classic(
    words: &mut Tokens<'_>,
    (channel, id, direction): (u16, &[u8], Direction),
    (remote, base): (bool, Base),
    data: &mut Vec<u8>,
) -> Shape {
    let Some(id) = parse_id(id, base) else {
        return Shape::Malformed(BAD_ID);
    };
    let mut head = Head {
        channel,
        id: Some(id),
        direction: Some(direction),
        kind: FrameKind::Can,
        dlc: 0,
    };
    if remote {
        head.kind = FrameKind::Remote;
        head.dlc = small(words.next(), radix(base), 15).unwrap_or(0);
        return Shape::Frame(head);
    }
    let Some(dlc) = small(words.next(), radix(base), 15) else {
        return Shape::Malformed(BAD_DLC);
    };
    head.dlc = dlc;
    match bytes(words, (dlc.min(8), base), data) {
        Some(()) => Shape::Frame(head),
        None => Shape::Malformed(BAD_BYTE),
    }
}

fn is_bit(token: Option<&[u8]>) -> bool {
    matches!(token, Some(b"0" | b"1"))
}

fn fd(words: &mut Tokens<'_>, base: Base, data: &mut Vec<u8>) -> Shape {
    let Some(channel) = words.next().and_then(|token| number(token, 10)) else {
        return Shape::Other;
    };
    let channel = u16::try_from(channel).unwrap_or(u16::MAX);
    let (Some(direction), Some(id)) = (direction(words.next()), words.next()) else {
        return Shape::Other;
    };
    if is_error_frame(id) {
        return error_frame(channel, Some(direction));
    }
    let Some(id) = parse_id(id, base) else {
        return Shape::Malformed(BAD_ID);
    };
    let mut look = words.clone();
    if !(is_bit(look.next()) && is_bit(look.next())) {
        words.next();
    }
    let flags = (small(words.next(), 2, 1), small(words.next(), 2, 1));
    let dlc = small(words.next(), radix(base), 15);
    let len = small(words.next(), 10, 64);
    let (Some(brs), Some(esi), Some(dlc), Some(len)) = (flags.0, flags.1, dlc, len) else {
        return Shape::Malformed(BAD_FD);
    };
    if bytes(words, (len, base), data).is_none() {
        return Shape::Malformed(BAD_BYTE);
    }
    Shape::Frame(Head {
        channel,
        id: Some(id),
        direction: Some(direction),
        kind: FrameKind::Fd {
            brs: brs == 1,
            esi: esi == 1,
        },
        dlc,
    })
}

fn after_time(words: &mut Tokens<'_>, base: Base, data: &mut Vec<u8>) -> Shape {
    let Some(second) = words.next() else {
        return Shape::Other;
    };
    if second.eq_ignore_ascii_case(b"CANFD") {
        return fd(words, base, data);
    }
    let Some(channel) = number(second, 10) else {
        return Shape::Other;
    };
    let channel = u16::try_from(channel).unwrap_or(u16::MAX);
    let Some(third) = words.next() else {
        return Shape::Other;
    };
    if is_error_frame(third) {
        return error_frame(channel, None);
    }
    let mut look = words.clone();
    match (direction(look.next()), look.next()) {
        (Some(direction), Some(marker @ (b"d" | b"r"))) => {
            let remote = marker == b"r";
            classic(&mut look, (channel, third, direction), (remote, base), data)
        }
        _ => Shape::Other,
    }
}

pub(crate) fn parse(line: &[u8], base: Base, data: &mut Vec<u8>) -> (Option<u64>, Shape) {
    data.clear();
    let line = line
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(line)
        .trim_ascii();
    if line.is_empty() {
        return (None, Shape::Blank);
    }
    if let Some(header) = header(line) {
        return (None, Shape::Header(header));
    }
    let mut words = tokens(line);
    let time = words.next().and_then(parse_time);
    if time.is_none() {
        return (None, Shape::Other);
    }
    (time, after_time(&mut words, base, data))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Clock {
    pub notation: Notation,
    pub now: u64,
}

impl Clock {
    pub(crate) fn tick(&mut self, time: Option<u64>) -> Option<u64> {
        let time = time?;
        self.now = if self.notation.relative {
            self.now.saturating_add(time)
        } else {
            time
        };
        Some(self.now)
    }

    pub(crate) fn read(&mut self, header: &Header) {
        if let Header::Notation { base, relative } = header {
            self.notation.base = base.unwrap_or(self.notation.base);
            self.notation.relative = relative.unwrap_or(self.notation.relative);
        }
    }
}
