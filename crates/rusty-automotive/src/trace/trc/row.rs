use std::str::SplitAsciiWhitespace;

use super::{BAD_COLUMNS, BAD_VERSION, Columns, State, Version};
use crate::trace::asc::{BAD_ID, Head, Shape};
use crate::trace::{CanId, Direction, FrameKind};

const BAD_ROW: &str = "the TRC row has missing or invalid fields";
const BAD_LENGTH: &str = "the TRC length or DLC is invalid for CAN";
const BAD_BYTES: &str = "a TRC data byte is missing or invalid";
const LENGTHS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

type Result<T> = std::result::Result<T, &'static str>;

struct Row<'a> {
    channel: u16,
    kind: &'a str,
    id: Option<&'a str>,
    direction: Option<Direction>,
    length: Option<&'a str>,
    dlc: bool,
    data: SplitAsciiWhitespace<'a>,
}

pub(super) fn parse(state: &State, text: &str, data: &mut Vec<u8>) -> (Option<u64>, Shape) {
    let mut words = text.split_ascii_whitespace();
    let row = match state.version {
        Version::V11 | Version::V13 => legacy(state.version, &mut words),
        Version::V20 | Version::V21 => match state.columns {
            Some(columns) => modern((state.version, columns), &mut words),
            None => Err(BAD_COLUMNS),
        },
        Version::Unknown => Err(BAD_VERSION),
    };
    match row {
        Ok((time, row)) => (
            Some(time),
            decode(row, data).unwrap_or_else(Shape::Malformed),
        ),
        Err(reason) => (None, Shape::Malformed(reason)),
    }
}

fn take<'a>(words: &mut SplitAsciiWhitespace<'a>) -> Result<&'a str> {
    words.next().ok_or(BAD_ROW)
}

fn number(words: &mut SplitAsciiWhitespace<'_>) -> Result<()> {
    let token = take(words)?.trim_end_matches(')');
    token.parse::<u64>().map(|_| ()).map_err(|_| BAD_ROW)
}

fn time(words: &mut SplitAsciiWhitespace<'_>) -> Result<u64> {
    let token = take(words)?;
    let (whole, fraction) = token.split_once('.').unwrap_or((token, ""));
    if whole.is_empty()
        || !token.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(BAD_ROW);
    }
    let millis: u64 = whole.parse().map_err(|_| BAD_ROW)?;
    let nanos = fraction
        .bytes()
        .take(6)
        .enumerate()
        .fold(0u64, |sum, (at, byte)| {
            sum + u64::from(byte - b'0') * 10u64.pow(5 - at as u32)
        });
    millis
        .checked_mul(1_000_000)
        .and_then(|ms| ms.checked_add(nanos))
        .ok_or(BAD_ROW)
}

fn direction(value: &str) -> Result<Direction> {
    match value {
        "Rx" => Ok(Direction::Rx),
        "Tx" => Ok(Direction::Tx),
        _ => Err(BAD_ROW),
    }
}

fn channel(words: &mut SplitAsciiWhitespace<'_>) -> Result<u16> {
    take(words)?
        .parse::<u16>()
        .ok()
        .filter(|bus| (1..=16).contains(bus))
        .ok_or(BAD_ROW)
}

fn legacy<'a>(version: Version, words: &mut SplitAsciiWhitespace<'a>) -> Result<(u64, Row<'a>)> {
    number(words)?;
    let time = time(words)?;
    let channel = if version == Version::V13 {
        channel(words)?
    } else {
        1
    };
    let kind = take(words)?;
    let id = Some(take(words)?);
    if version == Version::V13 {
        take(words)?;
    }
    let length = Some(take(words)?);
    let direction = if matches!(kind, "Error" | "Warng") {
        None
    } else {
        Some(direction(kind)?)
    };
    Ok((
        time,
        Row {
            channel,
            kind,
            id,
            direction,
            length,
            dlc: true,
            data: words.clone(),
        },
    ))
}

fn modern<'a>(
    (version, columns): (Version, Columns),
    words: &mut SplitAsciiWhitespace<'a>,
) -> Result<(u64, Row<'a>)> {
    if columns.number {
        number(words)?;
    }
    let time = time(words)?;
    let kind = take(words)?;
    if kind == "EV" {
        return Ok((
            time,
            Row {
                channel: 1,
                kind,
                id: None,
                direction: None,
                length: None,
                dlc: false,
                data: words.clone(),
            },
        ));
    }
    Ok((time, modern_fields((version, columns), kind, words)?))
}

fn modern_fields<'a>(
    (version, columns): (Version, Columns),
    kind: &'a str,
    words: &mut SplitAsciiWhitespace<'a>,
) -> Result<Row<'a>> {
    let channel = if columns.bus { channel(words)? } else { 1 };
    let event = matches!(kind, "ER" | "EC" | "ST");
    let id = if event && version == Version::V20 {
        None
    } else {
        Some(take(words)?)
    };
    let direction = Some(direction(take(words)?)?);
    if columns.reserved {
        take(words)?;
    }
    let length = if event && version == Version::V20 {
        None
    } else {
        Some(take(words)?)
    };
    Ok(Row {
        channel,
        kind,
        id,
        direction,
        length,
        dlc: columns.dlc,
        data: words.clone(),
    })
}

fn identifier(value: &str) -> Result<CanId> {
    if !(1..=8).contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(BAD_ID);
    }
    let raw = u32::from_str_radix(value, 16).map_err(|_| BAD_ID)?;
    let extended = value.len() > 4 || raw > 0x7FF;
    let limit = if extended {
        (1 << 29) - 1
    } else {
        (1 << 11) - 1
    };
    if raw > limit {
        return Err(BAD_ID);
    }
    Ok(CanId { raw, extended })
}

fn length(row: &Row<'_>, kind: FrameKind) -> Result<(u8, u8)> {
    let expected = match row.kind {
        "ER" => 5,
        "EC" => 2,
        "ST" => 4,
        _ => return parse_length(row, kind),
    };
    let count = row.length.map_or(Ok(expected), |value| {
        value.parse::<u8>().map_err(|_| BAD_LENGTH)
    })?;
    if count != expected {
        return Err(BAD_LENGTH);
    }
    Ok((count, count))
}

fn parse_length(row: &Row<'_>, kind: FrameKind) -> Result<(u8, u8)> {
    let count: u8 = row
        .length
        .ok_or(BAD_LENGTH)?
        .parse()
        .map_err(|_| BAD_LENGTH)?;
    let fd = matches!(kind, FrameKind::Fd { .. });
    if !fd {
        return if count <= 8 {
            Ok((count, count))
        } else {
            Err(BAD_LENGTH)
        };
    }
    if row.dlc {
        LENGTHS
            .get(usize::from(count))
            .copied()
            .map(|len| (count, len))
            .ok_or(BAD_LENGTH)
    } else {
        LENGTHS
            .iter()
            .position(|len| *len == count)
            .map(|dlc| (dlc as u8, count))
            .ok_or(BAD_LENGTH)
    }
}

fn decode(mut row: Row<'_>, data: &mut Vec<u8>) -> Result<Shape> {
    let kind = match row.kind {
        "DT" | "Rx" | "Tx" => FrameKind::Can,
        "FD" | "FB" | "FE" | "BI" => FrameKind::Fd {
            brs: matches!(row.kind, "FB" | "BI"),
            esi: matches!(row.kind, "FE" | "BI"),
        },
        "RR" => FrameKind::Remote,
        "ER" | "Error" => FrameKind::Error,
        "ST" | "EC" | "Warng" => {
            let (_, count) = length(&row, FrameKind::Can)?;
            read_bytes(&mut row.data, count, data)?;
            return Ok(Shape::Other);
        }
        _ => return Ok(Shape::Other),
    };
    let (dlc, count) = length(&row, kind)?;
    let legacy_remote = matches!(row.kind, "Rx" | "Tx") && row.data.clone().next() == Some("RTR");
    let remote = matches!(kind, FrameKind::Remote) || legacy_remote;
    let id = if kind == FrameKind::Error {
        None
    } else {
        Some(identifier(row.id.ok_or(BAD_ID)?)?)
    };
    if legacy_remote {
        row.data.next();
    } else if !remote {
        read_bytes(&mut row.data, count, data)?;
    }
    if row.data.next().is_some() {
        return Err(BAD_BYTES);
    }
    Ok(Shape::Frame(Head {
        channel: row.channel,
        id,
        direction: row.direction,
        kind: if remote { FrameKind::Remote } else { kind },
        dlc,
    }))
}

fn read_bytes(words: &mut SplitAsciiWhitespace<'_>, count: u8, data: &mut Vec<u8>) -> Result<()> {
    for _ in 0..count {
        let value = words
            .next()
            .filter(|value| value.len() == 2 && value.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(BAD_BYTES)?;
        data.push(u8::from_str_radix(value, 16).map_err(|_| BAD_BYTES)?);
    }
    Ok(())
}
