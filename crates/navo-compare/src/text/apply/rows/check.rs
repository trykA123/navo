use std::ops::Range;

use crate::text::diff::Row;

use super::super::ApplyError;
use super::{Plan, line_range};

#[derive(Default)]
struct Count {
    target: Option<usize>,
    source: Option<usize>,
    target_count: usize,
    source_count: usize,
    total: usize,
}

pub(super) fn check_rows(plan: &Plan<'_>) -> Result<usize, ApplyError> {
    let source_left = plan.direction.source_is_left();
    let target_side = if source_left {
        &plan.entry.hunk.right_lines
    } else {
        &plan.entry.hunk.left_lines
    };
    let source_side = if source_left {
        &plan.entry.hunk.left_lines
    } else {
        &plan.entry.hunk.right_lines
    };
    let mut count = Count::default();
    for row in plan.entry.rows {
        count_row(plan, *row, &mut count)?;
    }
    if count.target_count != target_side.len() || count.source_count != source_side.len() {
        return Err(ApplyError::Range);
    }
    if !target_side.is_empty() && count.target != Some(target_side.end - 1) {
        return Err(ApplyError::Range);
    }
    if !source_side.is_empty() && count.source != Some(source_side.end - 1) {
        return Err(ApplyError::Range);
    }
    Ok(count.total)
}

fn count_row(plan: &Plan<'_>, row: Row, count: &mut Count) -> Result<(), ApplyError> {
    match row {
        Row::Changed { left, right } => count_changed(plan, left, right, count),
        Row::Deleted { left } => count_deleted(plan, left, count),
        Row::Inserted { right } => count_inserted(plan, right, count),
        Row::Equal { .. } => Err(ApplyError::Range),
    }
}

fn count_changed(
    plan: &Plan<'_>,
    left: usize,
    right: usize,
    count: &mut Count,
) -> Result<(), ApplyError> {
    if !plan.entry.hunk.left_lines.contains(&left) || !plan.entry.hunk.right_lines.contains(&right)
    {
        return Err(ApplyError::Range);
    }
    let source_left = plan.direction.source_is_left();
    let (target, source) = if source_left {
        (right, left)
    } else {
        (left, right)
    };
    count_target(plan, target, count)?;
    count_source(plan, source, count)?;
    let empty = line_range(plan.starts.target(plan.direction), target)?.is_empty();
    if plan.lines.contains(&source) || !empty {
        count.total += 1;
    }
    Ok(())
}

fn count_deleted(plan: &Plan<'_>, left: usize, count: &mut Count) -> Result<(), ApplyError> {
    if !plan.entry.hunk.left_lines.contains(&left) {
        return Err(ApplyError::Range);
    }
    if plan.direction.source_is_left() {
        count_source(plan, left, count)?;
        if plan.lines.contains(&left) {
            count.total += 1;
        }
        return Ok(());
    }
    count_target(plan, left, count)?;
    if !line_range(plan.starts.target(plan.direction), left)?.is_empty() {
        count.total += 1;
    }
    Ok(())
}

fn count_inserted(plan: &Plan<'_>, right: usize, count: &mut Count) -> Result<(), ApplyError> {
    if !plan.entry.hunk.right_lines.contains(&right) {
        return Err(ApplyError::Range);
    }
    if !plan.direction.source_is_left() {
        count_source(plan, right, count)?;
        if plan.lines.contains(&right) {
            count.total += 1;
        }
        return Ok(());
    }
    count_target(plan, right, count)?;
    if !line_range(plan.starts.target(plan.direction), right)?.is_empty() {
        count.total += 1;
    }
    Ok(())
}

fn count_target(plan: &Plan<'_>, index: usize, count: &mut Count) -> Result<(), ApplyError> {
    let line = line_range(plan.starts.target(plan.direction), index)?;
    inside(&line, target_bytes(plan))?;
    if count.target.is_some_and(|previous| index <= previous) {
        return Err(ApplyError::Range);
    }
    count.target = Some(index);
    count.target_count += 1;
    Ok(())
}

fn count_source(plan: &Plan<'_>, index: usize, count: &mut Count) -> Result<(), ApplyError> {
    let line = line_range(plan.starts.source(plan.direction), index)?;
    inside(&line, source_bytes(plan))?;
    if count.source.is_some_and(|previous| index <= previous) {
        return Err(ApplyError::Range);
    }
    count.source = Some(index);
    count.source_count += 1;
    Ok(())
}

fn target_bytes<'a>(plan: &'a Plan<'_>) -> &'a Range<u64> {
    if plan.direction.source_is_left() {
        &plan.entry.hunk.right_bytes
    } else {
        &plan.entry.hunk.left_bytes
    }
}

fn source_bytes<'a>(plan: &'a Plan<'_>) -> &'a Range<u64> {
    if plan.direction.source_is_left() {
        &plan.entry.hunk.left_bytes
    } else {
        &plan.entry.hunk.right_bytes
    }
}

fn inside(line: &Range<u64>, hunk: &Range<u64>) -> Result<(), ApplyError> {
    if line.start < hunk.start || line.end > hunk.end {
        return Err(ApplyError::Range);
    }
    Ok(())
}
