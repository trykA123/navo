use super::{Chip, ChipKind, MapLine, Mdf, MdfProblem, Space};

const MAX_LINES: usize = 10_000;

#[must_use]
pub fn parse_mdf(text: &str) -> Mdf {
    let mut model = Mdf::default();
    let mut map_lines = Vec::new();
    for (at, text) in text.lines().take(MAX_LINES + 1).enumerate() {
        let line = at as u64 + 1;
        if at == MAX_LINES {
            model.problems.push(MdfProblem::CapReached {
                line,
                limit: "lines",
            });
            break;
        }
        let fields: Vec<_> = text
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .take(7)
            .collect();
        if fields.is_empty() {
            continue;
        }
        match parse_line(&mut model, &fields) {
            Ok(count) => {
                if fields[0] == "map" {
                    map_lines.push(line);
                }
                if fields.len() > count {
                    model.problems.push(MdfProblem::Malformed {
                        line,
                        reason: "extra fields ignored",
                    });
                }
            }
            Err(reason) => model.problems.push(MdfProblem::Malformed { line, reason }),
        }
    }
    validate_maps(&mut model, &map_lines);
    model.problems.sort_by_key(|problem| match problem {
        MdfProblem::Malformed { line, .. } | MdfProblem::CapReached { line, .. } => *line,
    });
    model
}

#[must_use]
pub fn sniff_mdf(head: &[u8]) -> bool {
    if head.starts_with(b"MDF     ") {
        return false;
    }
    let first = head
        .split(|byte| *byte == b'\n')
        .take(MAX_LINES)
        .map(|line| {
            line.split(|byte| *byte == b'#')
                .next()
                .unwrap_or_default()
                .trim_ascii()
        })
        .find(|line| !line.is_empty());
    matches!(
        first.and_then(|line| line.split(u8::is_ascii_whitespace).next()),
        Some(b"space" | b"chip" | b"map")
    )
}

fn parse_line(model: &mut Mdf, fields: &[&str]) -> Result<usize, &'static str> {
    match fields {
        ["space", name, id, mau, ..] => {
            let space = Space {
                name: (*name).to_owned(),
                id: number(id)?,
                mau: number(mau)?,
            };
            if model.spaces.iter().any(|previous| previous.id == space.id) {
                return Err("duplicate space id");
            }
            model.spaces.push(space);
            Ok(4)
        }
        ["chip", name, id, mau, size, kind, ..] => {
            let chip = Chip {
                name: (*name).to_owned(),
                id: number(id)?,
                mau: number(mau)?,
                size: number(size)?,
                kind: chip_kind(kind)?,
            };
            if model.chips.iter().any(|previous| previous.id == chip.id) {
                return Err("duplicate chip id");
            }
            model.chips.push(chip);
            Ok(6)
        }
        ["map", space, start, size, chip, chip_start, ..] => {
            model.maps.push(MapLine {
                space: number(space)?,
                start: number(start)?,
                size: number(size)?,
                chip: number(chip)?,
                chip_start: number(chip_start)?,
            });
            Ok(6)
        }
        _ => Err("unrecognized or incomplete definition"),
    }
}

fn chip_kind(text: &str) -> Result<ChipKind, &'static str> {
    match text {
        "rom" => Ok(ChipKind::Rom),
        "ram" => Ok(ChipKind::Ram),
        _ => Err("chip type must be rom or ram"),
    }
}

fn number(text: &str) -> Result<u64, &'static str> {
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .map_or_else(|| text.parse(), |hex| u64::from_str_radix(hex, 16))
        .map_err(|_| "invalid decimal or hexadecimal number")
}

fn validate_maps(model: &mut Mdf, lines: &[u64]) {
    let mut problems = Vec::new();
    let mut ranges = Vec::new();
    for (map, &line) in model.maps.iter().zip(lines) {
        if let Err(reason) = model.mapped_chip(map) {
            problems.push(MdfProblem::Malformed { line, reason });
        }
        if let Some(end) = map.start.checked_add(map.size).filter(|_| map.size > 0) {
            ranges.push((map.space, map.start, end, line));
        }
    }
    ranges.sort_unstable();
    let mut previous: Option<(u64, u64)> = None;
    for (space, start, end, line) in ranges {
        let earlier_end = previous.filter(|(id, _)| *id == space).map(|(_, end)| end);
        if earlier_end.is_some_and(|end| start < end) {
            problems.push(MdfProblem::Malformed {
                line,
                reason: "overlapping maps in address space",
            });
        }
        previous = Some((space, earlier_end.map_or(end, |earlier| earlier.max(end))));
    }
    model.problems.extend(problems);
}
