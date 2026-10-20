use super::fields::{Control, FIELDS, FieldSpec, Section};
use super::values::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nudge {
    Back,
    Forward,
    Flip,
}

pub fn nudged(spec: &FieldSpec, value: &Value, nudge: Nudge) -> Option<Value> {
    match (spec.control, value) {
        (Control::Choices(options, _), Value::Pick(at)) => {
            let last = options.len().checked_sub(1)?;
            let to = match nudge {
                Nudge::Back => at.checked_sub(1)?,
                Nudge::Forward => (*at < last).then_some(at + 1)?,
                Nudge::Flip if *at >= last => 0,
                Nudge::Flip => at + 1,
            };
            Some(Value::Pick(to))
        }
        (Control::Toggle, Value::Flag(on)) => {
            let to = match nudge {
                Nudge::Back => false,
                Nudge::Forward => true,
                Nudge::Flip => !on,
            };
            (to != *on).then_some(Value::Flag(to))
        }
        (Control::Number, Value::Number(number)) => {
            let to = match nudge {
                Nudge::Back => number - 1,
                Nudge::Forward | Nudge::Flip => number + 1,
            };
            (1..=i64::from(u16::MAX))
                .contains(&to)
                .then_some(Value::Number(to))
        }
        _ => None,
    }
}

pub fn moved(row: usize, down: bool) -> usize {
    if down {
        (row + 1).min(FIELDS.len() - 1)
    } else {
        row.saturating_sub(1)
    }
}

pub fn section_of(row: usize) -> Section {
    FIELDS
        .get(row)
        .map_or(Section::Appearance, |spec| spec.section)
}

pub fn first_row(section: Section) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.section == section)
        .unwrap_or(0)
}

pub fn next_section(row: usize, forward: bool) -> usize {
    let count = Section::ALL.len();
    let at = Section::ALL
        .iter()
        .position(|section| *section == section_of(row))
        .unwrap_or(0);
    let to = if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    };
    first_row(Section::ALL[to])
}

pub fn rows_in(section: Section) -> impl Iterator<Item = (usize, &'static FieldSpec)> {
    FIELDS
        .iter()
        .enumerate()
        .filter(move |(_, spec)| spec.section == section)
}

pub fn child_of(row: usize) -> Option<usize> {
    let section = section_of(row);
    let mut group = None;
    let mut headings = 0;
    for (at, (index, spec)) in rows_in(section).enumerate() {
        if section == Section::Display && spec.group() != group {
            group = spec.group();
            headings += usize::from(group.is_some());
        }
        if index == row {
            return Some(at + headings + 1);
        }
    }
    None
}
