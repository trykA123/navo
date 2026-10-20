use super::header::{ObjectHeader, u16_at, u32_at};
use super::{Error, Result};
use crate::{CanId, Direction, Frame, FrameKind};

pub(super) fn is_can(kind: u32) -> bool {
    matches!(kind, 1 | 73 | 86 | 100 | 101)
}

pub(super) fn decode(head: ObjectHeader, bytes: &[u8]) -> Result<Frame> {
    let frame = match head.kind {
        1 | 86 => classic(head.kind, bytes),
        73 => error_frame(bytes),
        100 => fd(bytes),
        101 => fd64(bytes),
        _ => Err(Error::Invalid("unsupported BLF CAN object")),
    }?;
    if frame.channel == 0 {
        return Err(Error::Invalid("invalid BLF CAN channel"));
    }
    Ok(frame)
}

fn id(raw: u32) -> Result<CanId> {
    let extended = raw & (1 << 31) != 0;
    let raw = raw & !(1 << 31);
    let limit = if extended {
        (1 << 29) - 1
    } else {
        (1 << 11) - 1
    };
    if raw > limit {
        return Err(Error::Invalid("invalid BLF CAN identifier"));
    }
    Ok(CanId { raw, extended })
}

fn byte(bytes: &[u8], at: usize) -> Result<u8> {
    bytes
        .get(at)
        .copied()
        .ok_or(Error::Invalid("short BLF CAN object"))
}

fn payload(bytes: &[u8], at: usize, length: u8) -> Result<Vec<u8>> {
    let end = at + usize::from(length);
    Ok(bytes
        .get(at..end)
        .ok_or(Error::Invalid("short BLF CAN payload"))?
        .to_vec())
}

fn direction(tx: bool) -> Option<Direction> {
    Some(if tx { Direction::Tx } else { Direction::Rx })
}

fn classic(kind: u32, bytes: &[u8]) -> Result<Frame> {
    let minimum = if kind == 86 { 24 } else { 16 };
    if bytes.len() < minimum {
        return Err(Error::Invalid("short BLF CAN message"));
    }
    let flags = byte(bytes, 2)?;
    let dlc = byte(bytes, 3)?;
    if dlc > 15 {
        return Err(Error::Invalid("invalid BLF classic DLC"));
    }
    let remote = flags & 128 != 0;
    Ok(Frame {
        channel: u16_at(bytes, 0)?,
        id: Some(id(u32_at(bytes, 4)?)?),
        direction: direction(flags & 1 != 0),
        kind: if remote {
            FrameKind::Remote
        } else {
            FrameKind::Can
        },
        dlc,
        data: if remote {
            Vec::new()
        } else {
            payload(bytes, 8, dlc.min(8))?
        },
    })
}

fn fd_kind(flags: u32, remote: bool) -> Result<FrameKind> {
    if remote && flags & 1 != 0 {
        return Err(Error::Invalid("BLF FD frame cannot be remote"));
    }
    Ok(if remote {
        FrameKind::Remote
    } else if flags & 1 != 0 {
        FrameKind::Fd {
            brs: flags & 2 != 0,
            esi: flags & 4 != 0,
        }
    } else {
        FrameKind::Can
    })
}

fn validate_length(dlc: u8, length: u8, kind: FrameKind) -> Result<()> {
    const LENGTHS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
    let maximum = LENGTHS
        .get(usize::from(dlc))
        .copied()
        .ok_or(Error::Invalid("invalid BLF CAN DLC"))?;
    let maximum = if matches!(kind, FrameKind::Fd { .. }) {
        maximum
    } else {
        maximum.min(8)
    };
    if length > maximum {
        return Err(Error::Invalid("invalid BLF valid payload length"));
    }
    Ok(())
}

fn fd(bytes: &[u8]) -> Result<Frame> {
    if bytes.len() < 84 {
        return Err(Error::Invalid("short BLF FD message"));
    }
    let flags = byte(bytes, 2)?;
    let dlc = byte(bytes, 3)?;
    let length = byte(bytes, 14)?;
    let kind = fd_kind(u32::from(byte(bytes, 13)?), flags & 128 != 0)?;
    validate_length(dlc, length, kind)?;
    Ok(Frame {
        channel: u16_at(bytes, 0)?,
        id: Some(id(u32_at(bytes, 4)?)?),
        direction: direction(flags & 1 != 0),
        kind,
        dlc,
        data: if kind == FrameKind::Remote {
            Vec::new()
        } else {
            payload(bytes, 20, length)?
        },
    })
}

fn fd64(bytes: &[u8]) -> Result<Frame> {
    let dlc = byte(bytes, 1)?;
    let length = byte(bytes, 2)?;
    let flags = u32_at(bytes, 12)?;
    let kind = fd_kind(flags >> 12, flags & 16 != 0)?;
    validate_length(dlc, length, kind)?;
    let logged = match byte(bytes, 34)? {
        0 => Direction::Rx,
        1 => Direction::Tx,
        2 => Direction::TxRequest,
        _ => return Err(Error::Invalid("invalid BLF CAN direction")),
    };
    if bytes.len() < 40 {
        return Err(Error::Invalid("short BLF FD message"));
    }
    Ok(Frame {
        channel: u16::from(byte(bytes, 0)?),
        id: Some(id(u32_at(bytes, 4)?)?),
        direction: Some(if flags & 128 != 0 {
            Direction::TxRequest
        } else {
            logged
        }),
        kind,
        dlc,
        data: if kind == FrameKind::Remote {
            Vec::new()
        } else {
            payload(bytes, 40, length)?
        },
    })
}

fn error_frame(bytes: &[u8]) -> Result<Frame> {
    let dlc = byte(bytes, 10)? & 15;
    let flags = u16_at(bytes, 20)?;
    let error_direction = if flags & 16_384 != 0 {
        Some(Direction::Tx)
    } else if u32_at(bytes, 4)? & 2 != 0 {
        direction(flags & 4096 != 0)
    } else if u32_at(bytes, 4)? & 1 != 0 {
        direction(byte(bytes, 8)? & 32 == 0)
    } else {
        None
    };
    Ok(Frame {
        channel: u16_at(bytes, 0)?,
        id: Some(id(u32_at(bytes, 16)?)?),
        direction: error_direction,
        kind: FrameKind::Error,
        dlc,
        data: payload(bytes, 24, dlc.min(8))?,
    })
}
