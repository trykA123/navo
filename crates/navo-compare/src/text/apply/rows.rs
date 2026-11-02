use std::collections::BTreeSet;
use std::ops::Range;

use navo_core::paging::ByteSource;

use super::super::diff::Row;
use super::source::{Copier, LineInput};
use super::target::{Target, Writer, copy_target};
use super::{ApplyError, Direction, Picked, PickedHunk, ranges};

mod check;

use check::check_rows;

pub(super) struct Rows<'a, 't> {
    pub(super) target: &'a Target<'t>,
    pub(super) source: &'a dyn ByteSource,
}

#[derive(Clone, Copy)]
struct Starts<'a> {
    left: &'a [u64],
    right: &'a [u64],
}

impl Starts<'_> {
    fn target(&self, direction: Direction) -> &[u64] {
        if direction.source_is_left() {
            self.right
        } else {
            self.left
        }
    }

    fn source(&self, direction: Direction) -> &[u64] {
        if direction.source_is_left() {
            self.left
        } else {
            self.right
        }
    }
}

struct Plan<'a> {
    entry: PickedHunk<'a>,
    starts: Starts<'a>,
    direction: Direction,
    lines: &'a BTreeSet<usize>,
    start: u64,
    end: u64,
}

struct State {
    cursor: u64,
    position: u64,
    emitted: usize,
    total: usize,
}

struct Out<'a, 't> {
    writer: &'a mut Writer<'t>,
    buffer: &'a mut [u8],
}

struct Run<'a, 'p> {
    plan: &'a Plan<'p>,
    state: State,
}

impl<'a, 't> Rows<'a, 't> {
    pub(super) fn validate(&self, picked: Picked<'_>) -> Result<(), ApplyError> {
        let mut cursor = self.target.bom_len;
        for entry in picked.hunks {
            let (plan, end) = self.plan(entry, &picked, cursor)?;
            check_rows(&plan)?;
            cursor = end;
        }
        Ok(())
    }

    pub(super) fn write(
        &self,
        picked: Picked<'_>,
        writer: &mut Writer<'t>,
        buffer: &mut [u8],
    ) -> Result<(), ApplyError> {
        let mut out = Out { writer, buffer };
        let mut cursor = self.target.bom_len;
        for entry in picked.hunks {
            let (plan, end) = self.plan(entry, &picked, cursor)?;
            copy_target(self.target, cursor..plan.start, out.writer, out.buffer)?;
            self.hunk(&plan, &mut out)?;
            cursor = end;
        }
        copy_target(self.target, cursor..self.target.len, out.writer, out.buffer)
    }

    fn plan<'e>(
        &self,
        entry: &PickedHunk<'e>,
        picked: &Picked<'e>,
        cursor: u64,
    ) -> Result<(Plan<'e>, u64), ApplyError> {
        let (target_range, source_range) = ranges(entry.hunk, picked.direction);
        let start = target_range.start.max(self.target.bom_len);
        let end = target_range.end.max(start);
        if start < cursor
            || end > self.target.len
            || source_range.end < source_range.start
            || source_range.end > self.source.len()
        {
            return Err(ApplyError::Range);
        }
        let plan = Plan {
            entry: *entry,
            starts: Starts {
                left: picked.left_starts,
                right: picked.right_starts,
            },
            direction: picked.direction,
            lines: picked.lines,
            start,
            end,
        };
        Ok((plan, end))
    }

    fn hunk(&self, plan: &Plan<'_>, out: &mut Out<'_, 't>) -> Result<(), ApplyError> {
        let total = check_rows(plan)?;
        let mut run = Run {
            plan,
            state: State {
                cursor: plan.start,
                position: plan.start,
                emitted: 0,
                total,
            },
        };
        for row in plan.entry.rows {
            self.row(&mut run, *row, out)?;
        }
        if run.plan.end < run.state.cursor {
            return Err(ApplyError::Range);
        }
        copy_target(
            self.target,
            run.state.cursor..run.plan.end,
            out.writer,
            out.buffer,
        )
    }

    fn row(
        &self,
        run: &mut Run<'_, '_>,
        row: Row,
        out: &mut Out<'_, 't>,
    ) -> Result<(), ApplyError> {
        let source_left = run.plan.direction.source_is_left();
        match row {
            Row::Changed { left, right } => self.changed(run, (left, right), out),
            Row::Deleted { left } if source_left => {
                if run.plan.lines.contains(&left) {
                    self.insert(run, left, out)?;
                }
                Ok(())
            }
            Row::Deleted { left } => self.target_only(run, left),
            Row::Inserted { right } if source_left => self.target_only(run, right),
            Row::Inserted { right } => {
                if run.plan.lines.contains(&right) {
                    self.insert(run, right, out)?;
                }
                Ok(())
            }
            Row::Equal { .. } => Err(ApplyError::Range),
        }
    }

    fn changed(
        &self,
        run: &mut Run<'_, '_>,
        pair: (usize, usize),
        out: &mut Out<'_, 't>,
    ) -> Result<(), ApplyError> {
        let plan = run.plan;
        let (left, right) = pair;
        let (target_index, source_index) = if plan.direction.source_is_left() {
            (right, left)
        } else {
            (left, right)
        };
        let target_line = line_range(plan.starts.target(plan.direction), target_index)?;
        let source_line = line_range(plan.starts.source(plan.direction), source_index)?;
        if plan.lines.contains(&source_index) {
            run.state.emitted += 1;
            self.copy_prefix(run, target_line.start, out)?;
            self.emit(run, source_line, out)?;
            run.state.cursor = target_line.end;
        } else if !target_line.is_empty() {
            run.state.emitted += 1;
        }
        run.state.position = target_line.end;
        Ok(())
    }

    fn target_only(&self, run: &mut Run<'_, '_>, index: usize) -> Result<(), ApplyError> {
        let plan = run.plan;
        let line = line_range(plan.starts.target(plan.direction), index)?;
        if line.start < run.state.cursor || line.end > plan.end {
            return Err(ApplyError::Range);
        }
        if !line.is_empty() {
            run.state.emitted += 1;
        }
        run.state.position = line.end;
        Ok(())
    }

    fn insert(
        &self,
        run: &mut Run<'_, '_>,
        index: usize,
        out: &mut Out<'_, 't>,
    ) -> Result<(), ApplyError> {
        let plan = run.plan;
        if run.state.position < run.state.cursor {
            return Err(ApplyError::Range);
        }
        copy_target(
            self.target,
            run.state.cursor..run.state.position,
            out.writer,
            out.buffer,
        )?;
        run.state.cursor = run.state.position;
        let line = line_range(plan.starts.source(plan.direction), index)?;
        run.state.emitted += 1;
        self.emit(run, line, out)
    }

    fn copy_prefix(
        &self,
        run: &mut Run<'_, '_>,
        start: u64,
        out: &mut Out<'_, 't>,
    ) -> Result<(), ApplyError> {
        let end = run.plan.end;
        if start < run.state.cursor || start > end {
            return Err(ApplyError::Range);
        }
        copy_target(self.target, run.state.cursor..start, out.writer, out.buffer)?;
        run.state.cursor = start;
        Ok(())
    }

    fn emit(
        &self,
        run: &mut Run<'_, '_>,
        line: Range<u64>,
        out: &mut Out<'_, 't>,
    ) -> Result<(), ApplyError> {
        let plan = run.plan;
        let copier = Copier::new(
            self.target,
            self.source,
            plan.end == self.target.len && !self.target.final_newline,
        );
        copier.line(
            LineInput {
                range: line,
                is_last: run.state.emitted == run.state.total,
            },
            out.writer,
            out.buffer,
        )
    }
}

fn line_range(starts: &[u64], index: usize) -> Result<Range<u64>, ApplyError> {
    let start = *starts.get(index).ok_or(ApplyError::Range)?;
    let end = *starts.get(index + 1).ok_or(ApplyError::Range)?;
    if end < start {
        return Err(ApplyError::Range);
    }
    Ok(start..end)
}
