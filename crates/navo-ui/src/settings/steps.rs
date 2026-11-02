use super::fields::{Control, FIELDS, Field, FieldSpec, Section};
use super::find::{Filter, Line, lines};
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
            let step = if spec.field == Field::UiScale { 10 } else { 1 };
            let to = match nudge {
                Nudge::Back => number.checked_sub(step)?,
                Nudge::Forward | Nudge::Flip => number.checked_add(step)?,
            };
            (1..=i64::from(u16::MAX))
                .contains(&to)
                .then_some(Value::Number(to))
        }
        _ => None,
    }
}

pub fn moved(filter: &Filter, row: usize, down: bool) -> usize {
    let mut admitted = FIELDS
        .iter()
        .enumerate()
        .filter(|(_, spec)| filter.admits(spec))
        .map(|(index, _)| index);
    let found = if down {
        admitted.find(|index| *index > row)
    } else {
        admitted.rfind(|index| *index < row)
    };
    found.unwrap_or(row)
}

pub const DIAGNOSTICS_ROW: usize = FIELDS.len();
pub const HELP_ROW: usize = FIELDS.len() + 1;

pub fn section_of(row: usize) -> Section {
    if row == DIAGNOSTICS_ROW {
        return Section::Diagnostics;
    }
    if row >= HELP_ROW {
        return Section::Help;
    }
    FIELDS
        .get(row)
        .map_or(Section::Appearance, |spec| spec.section)
}

pub fn first_row(section: Section) -> usize {
    match section {
        Section::Diagnostics => return DIAGNOSTICS_ROW,
        Section::Help => return HELP_ROW,
        _ => {}
    }
    FIELDS
        .iter()
        .position(|spec| spec.section == section)
        .unwrap_or(0)
}

pub fn next_section(filter: &Filter, row: usize, forward: bool) -> usize {
    let count = Section::ALL.len();
    let at = Section::ALL
        .iter()
        .position(|section| *section == section_of(row))
        .unwrap_or(0);
    (1..count)
        .map(|step| {
            if forward {
                at + step
            } else {
                at + count - step
            }
        })
        .find_map(|to| filter.first_in(Section::ALL[to % count]))
        .unwrap_or(row)
}

pub fn rows_in(section: Section) -> impl Iterator<Item = (usize, &'static FieldSpec)> {
    FIELDS
        .iter()
        .enumerate()
        .filter(move |(_, spec)| spec.section == section)
}

pub fn child_of(filter: &Filter, row: usize) -> Option<usize> {
    lines(filter, section_of(row))
        .iter()
        .position(|line| *line == Line::Row(row))
}
