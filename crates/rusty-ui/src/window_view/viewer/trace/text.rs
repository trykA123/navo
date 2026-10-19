use rusty_automotive::{
    Base, Body, CanId, Direction, Entry, Frame, FrameKind, IdStat, MAX_IDS, Trace,
};

use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    Error,
    Quiet,
    Bad,
}

pub(super) fn column_titles() -> String {
    format!(
        "{:>12}  {:<3} {:<11} {:<5} {:<7} {:>3}  {}",
        "Time", "Ch", "ID", "Dir", "Type", "DLC", "Data"
    )
}

pub(super) fn time(ns: u64) -> String {
    format!("{}.{:06}", ns / 1_000_000_000, ns % 1_000_000_000 / 1000)
}

pub(super) fn span(ns: u64) -> String {
    if ns < 1_000_000_000 {
        format!("{}.{:03} ms", ns / 1_000_000, ns % 1_000_000 / 1000)
    } else {
        format!(
            "{}.{:03} s",
            ns / 1_000_000_000,
            ns % 1_000_000_000 / 1_000_000
        )
    }
}

pub(super) fn id(id: CanId, base: Base) -> String {
    let suffix = if id.extended { "x" } else { "" };
    match (base, id.extended) {
        (Base::Dec, _) => format!("{}{suffix}", id.raw),
        (Base::Hex, true) => format!("{:08X}x", id.raw),
        (Base::Hex, false) => format!("{:03X}", id.raw),
    }
}

pub(super) fn direction(direction: Option<Direction>) -> &'static str {
    match direction {
        Some(Direction::Rx) => "Rx",
        Some(Direction::Tx) => "Tx",
        Some(Direction::TxRequest) => "TxRq",
        None => "",
    }
}

pub(super) fn kind(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Can => "CAN",
        FrameKind::Fd { .. } => "CAN FD",
        FrameKind::Remote => "Remote",
        FrameKind::Error => "Error",
    }
}

pub(super) fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn binary(byte: u8) -> String {
    format!("{:04b} {:04b}", byte >> 4, byte & 0x0F)
}

fn length(frame: &Frame) -> String {
    match frame.kind {
        FrameKind::Fd { .. } => frame.data.len().to_string(),
        FrameKind::Error => String::new(),
        FrameKind::Can | FrameKind::Remote => frame.dlc.to_string(),
    }
}

fn columns(time: &str, frame: Option<(&Frame, Base)>, (type_, data): (&str, &str)) -> String {
    let (channel, id_text, dir, len) = frame.map_or_else(Default::default, |(frame, base)| {
        let id_text = frame.id.map(|found| id(found, base)).unwrap_or_default();
        (
            frame.channel.to_string(),
            id_text,
            direction(frame.direction),
            length(frame),
        )
    });
    format!("{time:>12}  {channel:<3} {id_text:<11} {dir:<5} {type_:<7} {len:>3}  {data}")
}

fn without_time(text: &str) -> &str {
    let trimmed = text.trim_start();
    match trimmed.split_once(char::is_whitespace) {
        Some((first, rest)) if is_time_token(first) => rest.trim_start(),
        _ => trimmed,
    }
}

fn is_time_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

pub(super) fn row(entry: &Entry, base: Base) -> (String, Tone) {
    let at = entry.time_ns.map(time).unwrap_or_default();
    match &entry.body {
        Body::Frame(frame) => {
            let tone = if frame.kind == FrameKind::Error {
                Tone::Error
            } else {
                Tone::Plain
            };
            let data = hex(&frame.data);
            (
                columns(&at, Some((frame, base)), (kind(frame.kind), &data)),
                tone,
            )
        }
        Body::Other(text) => (
            columns(&at, None, ("Other", without_time(text))),
            Tone::Quiet,
        ),
        Body::Malformed { text, .. } => {
            (columns(&at, None, ("Bad", without_time(text))), Tone::Bad)
        }
    }
}

pub(super) fn stat_name(stat: &IdStat, base: Base) -> String {
    format!("{} · ch {}", id(stat.id, base), stat.channel)
}

pub(super) fn matches(stat: &IdStat, typed: &str, base: Base) -> bool {
    typed.is_empty()
        || id(stat.id, base)
            .to_ascii_lowercase()
            .contains(&typed.to_ascii_lowercase())
}

pub(super) fn period(stat: &IdStat) -> String {
    stat.mean_period_ns().map_or_else(|| "—".to_owned(), span)
}

pub(super) struct Check {
    pub text: String,
    pub is_bad: bool,
}

fn check(text: String, is_bad: bool) -> Check {
    Check { text, is_bad }
}

pub(super) fn status(trace: &Trace) -> Vec<Check> {
    let mut checks = vec![
        check(
            format::count(trace.kinds.frames(), "frame", "frames"),
            false,
        ),
        check(span(trace.duration_ns()), false),
        check(
            format::count(trace.channels.len() as u64, "channel", "channels"),
            false,
        ),
    ];
    let errors = trace.kinds.error;
    checks.push(check(
        format::count(errors, "error frame", "error frames"),
        errors > 0,
    ));
    if trace.unlisted > 0 {
        checks.push(check(
            format!("IDs beyond {} not listed", format::bytes(MAX_IDS as u64)),
            false,
        ));
    }
    if trace.malformed > 0 {
        checks.push(check(
            format::count(trace.malformed, "malformed line", "malformed lines"),
            true,
        ));
    }
    checks
}
