//! Keyboard steps in Settings: moving between rows and pages, and nudging a value.

use super::fields::{Control, FIELDS, Field, FieldSpec, Section};
use super::find::{Filter, Line, lines};
use super::pages::ordered_rows;
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
            let (low, high) = spec.field.number_range();
            (low..=high).contains(&to).then_some(Value::Number(to))
        }
        _ => None,
    }
}

/// The row one step up or down from `row` among the rows `filter` admits, in page order;
/// `row` itself at either end or when `row` is not one of them.
pub fn moved(filter: &Filter, row: usize, down: bool) -> usize {
    let admitted: Vec<usize> = ordered_rows()
        .filter(|(_, spec)| filter.admits(spec))
        .map(|(index, _)| index)
        .collect();
    let Some(at) = admitted.iter().position(|index| *index == row) else {
        return row;
    };
    let to = if down {
        at.checked_add(1)
    } else {
        at.checked_sub(1)
    };
    to.and_then(|to| admitted.get(to)).copied().unwrap_or(row)
}

pub const DIAGNOSTICS_ROW: usize = FIELDS.len();
pub const HELP_ROW: usize = FIELDS.len() + 1;

impl Field {
    /// The sheet row that shows this field; the Help toggle lives on the Help page row.
    pub fn row(self) -> usize {
        if self == Self::CheckUpdates {
            HELP_ROW
        } else {
            self.index()
        }
    }
}

/// The page that shows `row`; the Help and Diagnostics rows are on About.
pub fn section_of(row: usize) -> Section {
    if row >= DIAGNOSTICS_ROW {
        return Section::About;
    }
    FIELDS
        .get(row)
        .map_or(Section::Appearance, |spec| spec.section)
}

/// The first row of `page`: the Help row on About, else its first grouped setting.
pub fn first_row(page: Section) -> usize {
    if page == Section::About {
        return HELP_ROW;
    }
    rows_in(page).next().map_or(0, |(index, _)| index)
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

/// The grouped rows of `page` as `(row, spec)`, in the order the page lists them.
pub fn rows_in(page: Section) -> impl Iterator<Item = (usize, &'static FieldSpec)> {
    ordered_rows().filter(move |(_, spec)| spec.section == page)
}

pub fn child_of(filter: &Filter, row: usize) -> Option<usize> {
    lines(filter, section_of(row))
        .iter()
        .position(|line| *line == Line::Row(row))
}

/// The Ctrl+digit of a page: 1 to 9 in nav order, then 0 for the tenth page (About).
pub fn digit_of(page: Section) -> u8 {
    let position = Section::ALL.iter().position(|shown| *shown == page);
    position.map_or(0, |position| ((position + 1) % 10) as u8)
}
