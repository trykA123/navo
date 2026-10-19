use std::ops::Range;

use rusty_automotive::{Image, Item, Problem, Start};

use crate::format;

const WIDE: u64 = 0xFFFF_FFFF;

pub(super) fn address(at: u64, wide: bool) -> String {
    if wide {
        format!("0x{at:016X}")
    } else {
        format!("0x{at:08X}")
    }
}

pub(super) fn is_wide(image: &Image) -> bool {
    image.ranges.last().is_some_and(|last| last.end - 1 > WIDE)
}

pub(super) fn whole(image: &Image) -> Range<u64> {
    let first = image.ranges.first().map_or(0, |range| range.start);
    first..image.ranges.last().map_or(first, |range| range.end)
}

pub(super) fn span(range: &Range<u64>, wide: bool) -> String {
    format!(
        "{}–{}",
        address(range.start, wide),
        address(range.end.saturating_sub(1), wide)
    )
}

pub(super) fn hex_cells(bytes: &[Option<u8>]) -> (String, Vec<Range<usize>>) {
    let mut text = String::with_capacity(50);
    let mut missing = Vec::new();
    for (at, byte) in bytes.iter().enumerate() {
        if at > 0 {
            text.push_str(if at == 8 { "  " } else { " " });
        }
        match byte {
            Some(byte) => text.push_str(&format!("{byte:02X}")),
            None => {
                missing.push(text.len()..text.len() + 2);
                text.push_str("--");
            }
        }
    }
    (text, missing)
}

pub(super) fn ascii(bytes: &[Option<u8>]) -> String {
    bytes
        .iter()
        .map(|byte| match byte {
            Some(byte) if byte.is_ascii_graphic() || *byte == b' ' => char::from(*byte),
            Some(_) => '.',
            None => ' ',
        })
        .collect()
}

pub(super) fn gap_row(range: &Range<u64>, wide: bool) -> String {
    format!(
        "gap {} · {} not in file",
        span(range, wide),
        format::size(range.end - range.start)
    )
}

pub(super) fn item_name(items: &[Item], at: usize) -> String {
    match items.get(at) {
        Some(Item::Range(_)) => format!("Range {}", at / 2 + 1),
        Some(Item::Gap(_)) => "Gap".to_owned(),
        None => String::new(),
    }
}

pub(super) fn start(start: Start, wide: bool) -> String {
    match start {
        Start::Linear(at) => address(at, wide),
        Start::Segment { cs, ip } => format!("{cs:04X}:{ip:04X}"),
    }
}

pub(super) struct Check {
    pub text: String,
    pub is_bad: bool,
}

fn check(text: String, is_bad: bool) -> Check {
    Check { text, is_bad }
}

pub(super) fn status(image: &Image) -> Vec<Check> {
    let wide = is_wide(image);
    let gaps = image.ranges.len().saturating_sub(1) as u64;
    let mut checks = vec![check(
        format!(
            "{} · {}",
            format::count(image.ranges.len() as u64, "range", "ranges"),
            format::count(gaps, "gap", "gaps")
        ),
        false,
    )];
    checks.push(match image.checksum_errors {
        0 => check("checksum OK".to_owned(), false),
        errors => check(
            format::count(errors, "checksum error", "checksum errors"),
            true,
        ),
    });
    let other = image.problem_total - image.checksum_errors;
    if other > 0 {
        checks.push(check(
            format::count(other, "other problem", "other problems"),
            true,
        ));
    }
    checks.push(if image.has_end {
        check("EOF present".to_owned(), false)
    } else {
        check("EOF missing".to_owned(), true)
    });
    checks.push(match image.start {
        Some(at) => check(format!("start {}", start(at, wide)), false),
        None => check("no start address".to_owned(), false),
    });
    checks
}

pub(super) fn problem(problem: &Problem, wide: bool) -> (String, String) {
    match problem {
        Problem::Checksum {
            line,
            stored,
            computed,
        } => (
            format!("Checksum error · line {}", format::bytes(*line)),
            format!("record checksum 0x{stored:02X}, computed 0x{computed:02X}"),
        ),
        Problem::Malformed { line, reason } => (
            format!("Malformed record · line {}", format::bytes(*line)),
            (*reason).to_owned(),
        ),
        Problem::Overlap { line, range } => (
            format!("Overlap · line {}", format::bytes(*line)),
            format!("{} is written twice", span(range, wide)),
        ),
        Problem::TooFragmented { line } => (
            format!("Too fragmented · line {}", format::bytes(*line)),
            "indexing stopped here; the rest of the file is not shown".to_owned(),
        ),
        Problem::AfterEnd { line } => (
            format!("After the end · line {}", format::bytes(*line)),
            "records after the end-of-file record are ignored".to_owned(),
        ),
    }
}

pub(super) fn percent(part: u64, whole: u64) -> String {
    let share = part as f64 * 100. / whole.max(1) as f64;
    format!("{share:.1} %")
}

pub(super) fn parse_address(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).map_err(|_| format!("Not a hex address: {trimmed}"))
}
